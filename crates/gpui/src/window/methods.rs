use super::*;

impl Window {
    pub(crate) fn new(
        handle: AnyWindowHandle,
        options: WindowOptions,
        cx: &mut App,
    ) -> Result<Self> {
        let WindowOptions {
            window_bounds,
            titlebar,
            focus,
            show,
            kind,
            is_movable,
            app_owns_titlebar_drag,
            inactive_frame_interval,
            low_power_frame_interval,
            thermal_frame_interval,
            is_resizable,
            is_minimizable,
            display_id,
            window_background,
            app_id,
            window_min_size,
            window_decorations,
            #[cfg_attr(
                not(any(target_os = "linux", target_os = "freebsd")),
                allow(unused_variables)
            )]
            icon,
            #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
            tabbing_identifier,
            #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
            use_display_p3,
        } = options;

        let window_bounds = window_bounds.unwrap_or_else(|| default_bounds(display_id, cx));
        let mut platform_window = cx.platform.open_window(
            handle,
            WindowParams {
                bounds: window_bounds.get_bounds(),
                titlebar,
                kind,
                is_movable,
                app_owns_titlebar_drag,
                is_resizable,
                is_minimizable,
                focus,
                show,
                display_id,
                window_min_size,
                app_id: app_id.clone(),
                icon,
                #[cfg(target_os = "macos")]
                tabbing_identifier,
                #[cfg(target_os = "macos")]
                use_display_p3,
            },
        )?;

        let tab_bar_visible = platform_window.tab_bar_visible();
        SystemWindowTabController::init_visible(cx, tab_bar_visible);
        if let Some(tabs) = platform_window.tabbed_windows() {
            SystemWindowTabController::add_tab(cx, handle.window_id(), tabs);
        }

        let display_id = platform_window.display().map(|display| display.id());
        let sprite_atlas = platform_window.sprite_atlas();
        let mouse_position = platform_window.mouse_position();
        let modifiers = platform_window.modifiers();
        let capslock = platform_window.capslock();
        let content_size = platform_window.content_size();
        let scale_factor = platform_window.scale_factor();
        let appearance = platform_window.appearance();
        let text_system = Rc::new(WindowTextSystem::new(cx.text_system().clone()));
        #[cfg(not(feature = "frame-diagnostics"))]
        let invalidator = WindowInvalidator::new();
        #[cfg(feature = "frame-diagnostics")]
        let invalidator = WindowInvalidator::new(handle.window_id());
        let active = Rc::new(Cell::new(platform_window.is_active()));
        let hovered = Rc::new(Cell::new(platform_window.is_hovered()));
        let frame_scheduler = FrameScheduler::new();
        let needs_present = frame_scheduler.needs_present_handle();
        let next_frame_callbacks = frame_scheduler.next_frame_callbacks_handle();
        let input_rate_tracker = Rc::new(RefCell::new(InputRateTracker::default()));

        if platform_window.capabilities().window_services.decorations {
            platform_window
                .request_decorations(window_decorations.unwrap_or(WindowDecorations::Server));
        }
        platform_window.set_background_appearance(window_background);

        match window_bounds {
            WindowBounds::Fullscreen(_) => platform_window.toggle_fullscreen(),
            WindowBounds::Maximized(_) => {
                // The window's bounds are already applied through WindowParams;
                // zoom only asks the window manager to enter the maximized
                // state. Backends that cannot zoom (web, headless, test) treat
                // it as best-effort, so a failure must not abort window
                // creation. Log it and open the window anyway.
                platform_window.zoom().log_err();
            }
            WindowBounds::Windowed(_) => {}
        }

        #[cfg(feature = "accessibility")]
        if platform_window.capabilities().accessibility {
            enum AccessibilityEvent {
                Activated,
                Deactivated,
                Action(accesskit::ActionRequest),
            }

            let initial_tree = crate::SemanticTreeBuilder::new().snapshot().update;
            let (event_sender, mut event_receiver) = mpsc::unbounded();
            platform_window.initialize_accessibility(crate::AccessibilityCallbacks {
                activation: {
                    let event_sender = event_sender.clone();
                    Box::new(move || {
                        if let Err(error) =
                            event_sender.unbounded_send(AccessibilityEvent::Activated)
                        {
                            log::error!("failed to report accessibility activation: {error}");
                        }
                        Some(initial_tree.clone())
                    })
                },
                action: {
                    let event_sender = event_sender.clone();
                    Box::new(move |request| {
                        if let Err(error) =
                            event_sender.unbounded_send(AccessibilityEvent::Action(request))
                        {
                            log::error!("failed to route accessibility action: {error}");
                        }
                    })
                },
                deactivation: Box::new(move || {
                    if let Err(error) = event_sender.unbounded_send(AccessibilityEvent::Deactivated)
                    {
                        log::error!("failed to report accessibility deactivation: {error}");
                    }
                }),
            })?;
            platform_window.update_accessibility_window_bounds()?;

            let mut async_cx = cx.to_async();
            cx.foreground_executor()
                .spawn(async move {
                    while let Some(event) = event_receiver.next().await {
                        handle
                            .update(&mut async_cx, |_, window, cx| match event {
                                AccessibilityEvent::Activated | AccessibilityEvent::Deactivated => {
                                    window.refresh()
                                }
                                AccessibilityEvent::Action(request) => {
                                    window.dispatch_accessibility_action(
                                        request.target_node,
                                        request.action,
                                        request.data.as_ref(),
                                        cx,
                                    );
                                }
                            })
                            .log_err();
                    }
                })
                .detach();
        }

        platform_window.on_close(Box::new({
            let window_id = handle.window_id();
            let cx = cx.to_async();
            move || {
                // Native window destruction can re-enter while the app is borrowed.
                // Schedule the layout changes instead of borrowing the app here.
                cx.spawn(async move |cx| {
                    cx.update(|cx| {
                        // App-driven closure may already have removed the window.
                        if cx.windows.contains_key(window_id) {
                            handle
                                .update(cx, |_, window, _| window.remove_window())
                                .log_err();
                        }
                        SystemWindowTabController::remove_tab(cx, window_id);
                    });
                })
                .detach();
            }
        }));
        platform_window.on_request_frame(Box::new({
            let mut cx = cx.to_async();
            let invalidator = invalidator.clone();
            let active = active.clone();
            let input_rate_tracker = input_rate_tracker.clone();
            let mut deferred_force_render = false;
            let mut frame_pacer = FramePacer::default();
            move |request_frame_options| {
                let force_render =
                    mem::take(&mut deferred_force_render) || request_frame_options.force_render;
                let power_state = handle
                    .update(&mut cx, |_, _, cx| (cx.thermal_state(), cx.power_mode()))
                    .log_err();

                // Keep presenting briefly after high-rate input to prevent display
                // underclocking, but subject to the same power limits as redraws.
                let high_rate_input = input_rate_tracker.borrow_mut().is_high_rate();
                let has_next_frame_callbacks = !next_frame_callbacks.borrow().is_empty();
                let has_frame_demand = invalidator.is_dirty()
                    || force_render
                    || has_next_frame_callbacks
                    || request_frame_options.require_presentation
                    || needs_present.get()
                    || high_rate_input;

                let should_throttle = !request_frame_options.require_presentation;
                let inactive_without_high_rate_input =
                    should_throttle && !active.get() && !high_rate_input;
                let (thermal_state, power_mode) =
                    power_state.unwrap_or((ThermalState::Nominal, crate::PowerMode::Unknown));
                let min_frame_interval = minimum_frame_interval(
                    should_throttle,
                    inactive_without_high_rate_input,
                    inactive_frame_interval,
                    low_power_frame_interval,
                    thermal_frame_interval,
                    thermal_state,
                    power_mode,
                );

                let now = Instant::now();
                // Idle display-link ticks must neither consume a frame slot nor
                // wake a demand-driven frame source.
                if has_frame_demand && frame_pacer.should_defer(now, min_frame_interval) {
                    // Don't lose a pending forced render to throttling.
                    deferred_force_render |= force_render;
                    // Must still complete the frame on platforms that require it.
                    // On Wayland, `surface.frame()` was already called to request the
                    // next frame callback, so we must call `surface.commit()` (via
                    // `complete_frame`) or the compositor won't send another callback.
                    handle
                        .update(&mut cx, |_, window, _| window.complete_frame())
                        .log_err();
                    // The demand is still unserved; platforms that stop
                    // requesting frames for idle windows need a wakeup to retry.
                    invalidator.wake_platform();
                    return;
                }

                let pending_next_frame_callbacks = next_frame_callbacks.take();
                if !pending_next_frame_callbacks.is_empty() {
                    handle
                        .update(&mut cx, |_, window, cx| {
                            for callback in pending_next_frame_callbacks {
                                callback(window, cx);
                            }
                        })
                        .log_err();
                }

                let should_draw = invalidator.is_dirty() || force_render;
                let needs_present = request_frame_options.require_presentation
                    || needs_present.get()
                    || high_rate_input;

                if should_draw {
                    measure("frame duration", || {
                        handle
                            .update(&mut cx, |_, window, cx| {
                                if force_render {
                                    // Bypass cached view reuse so we don't replay stale
                                    // atlas tile references after a GPU device recovery.
                                    window.refresh();
                                }
                                let arena_clear_needed = window.draw(cx);
                                window.present();
                                arena_clear_needed.clear();
                            })
                            .log_err();
                    })
                } else if needs_present {
                    handle
                        .update(&mut cx, |_, window, _| window.present())
                        .log_err();
                }

                handle
                    .update(&mut cx, |_, window, _| {
                        window.complete_frame();
                    })
                    .log_err();

                if invalidator.is_dirty() || !next_frame_callbacks.borrow().is_empty() {
                    invalidator.wake_platform();
                }
            }
        }));
        invalidator.set_platform_waker(platform_window.frame_waker());
        platform_window.on_resize(Box::new({
            let mut cx = cx.to_async();
            move |_, _| {
                handle
                    .update(&mut cx, |_, window, cx| window.bounds_changed(cx))
                    .log_err();
            }
        }));
        platform_window.on_moved(Box::new({
            let mut cx = cx.to_async();
            move || {
                handle
                    .update(&mut cx, |_, window, cx| window.bounds_changed(cx))
                    .log_err();
            }
        }));
        platform_window.on_appearance_changed(Box::new({
            let cx = cx.to_async();
            let foreground_executor = cx.foreground_executor().clone();
            move || {
                let mut cx = cx.clone();
                // Defer the update because changing the AppKit appearance may
                // synchronously invoke this callback while App is already borrowed.
                foreground_executor
                    .spawn(async move {
                        handle
                            .update(&mut cx, |_, window, cx| window.appearance_changed(cx))
                            .log_err();
                    })
                    .detach();
            }
        }));
        platform_window.on_button_layout_changed(Box::new({
            let mut cx = cx.to_async();
            move || {
                handle
                    .update(&mut cx, |_, window, cx| window.button_layout_changed(cx))
                    .log_err();
            }
        }));
        platform_window.on_active_status_change(Box::new({
            let mut cx = cx.to_async();
            move |active| {
                log::info!("window active status changed: {active}");
                handle
                    .update(&mut cx, |_, window, cx| {
                        window.active.set(active);
                        window.interaction.modifiers = window.platform_window.modifiers();
                        window.interaction.capslock = window.platform_window.capslock();
                        window
                            .activation_observers
                            .clone()
                            .retain(&(), |callback| callback(window, cx));

                        window.bounds_changed(cx);
                        window.refresh();

                        SystemWindowTabController::update_last_active(cx, window.handle.id);
                    })
                    .log_err();
            }
        }));
        platform_window.on_hover_status_change(Box::new({
            let mut cx = cx.to_async();
            move |active| {
                handle
                    .update(&mut cx, |_, window, _| {
                        window.hovered.set(active);
                        window.refresh();
                    })
                    .log_err();
            }
        }));
        platform_window.on_input({
            let mut cx = cx.to_async();
            Box::new(move |event| {
                handle
                    .update(&mut cx, |_, window, cx| window.dispatch_event(event, cx))
                    .log_err()
                    .unwrap_or(DispatchEventResult::default())
            })
        });
        platform_window.on_hit_test_window_control({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, window, _cx| {
                        for (area, hitbox) in
                            &window.interaction.rendered_frame.window_control_hitboxes
                        {
                            if window.interaction.mouse_hit_test.ids.contains(&hitbox.id) {
                                return Some(*area);
                            }
                        }
                        None
                    })
                    .log_err()
                    .unwrap_or(None)
            })
        });
        platform_window.on_move_tab_to_new_window({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::move_tab_to_new_window(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_merge_all_windows({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::merge_all_windows(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_select_next_tab({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::select_next_tab(cx, handle.window_id());
                    })
                    .log_err();
            })
        });
        platform_window.on_select_previous_tab({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, _window, cx| {
                        SystemWindowTabController::select_previous_tab(cx, handle.window_id())
                    })
                    .log_err();
            })
        });
        platform_window.on_toggle_tab_bar({
            let mut cx = cx.to_async();
            Box::new(move || {
                handle
                    .update(&mut cx, |_, window, cx| {
                        let tab_bar_visible = window.platform_window.tab_bar_visible();
                        SystemWindowTabController::set_visible(cx, tab_bar_visible);
                    })
                    .log_err();
            })
        });

        if let Some(app_id) = app_id
            && platform_window.capabilities().window_services.app_id
        {
            platform_window.set_app_id(&app_id);
        }

        platform_window.map_window()?;

        Ok(Window {
            handle,
            invalidator,
            removed: false,
            platform_window,
            display_id,
            is_resizable,
            is_minimizable,
            sprite_atlas,
            text_system,
            text_rendering_mode: cx.text_rendering_mode.clone(),
            rem_size: px(16.),
            rem_size_override_stack: SmallVec::new(),
            viewport_size: content_size,
            root: None,
            frame_builder: FrameBuilder::new(),
            interaction: InteractionOwner::new(
                Frame::new(DispatchTree::new(cx.keymap.clone(), cx.actions.clone())),
                Frame::new(DispatchTree::new(cx.keymap.clone(), cx.actions.clone())),
                HitboxId(0),
                mouse_position,
                modifiers,
                capslock,
            ),
            text_input: TextInputOwner::new(),
            frame_scheduler,
            scale_factor,
            bounds_observers: SubscriberSet::new(),
            appearance,
            appearance_observers: SubscriberSet::new(),
            button_layout_observers: SubscriberSet::new(),
            active,
            hovered,
            input_rate_tracker,
            #[cfg(feature = "input-latency-histogram")]
            input_latency_tracker: InputLatencyTracker::new()?,
            activation_observers: SubscriberSet::new(),
            prompt: None,
            client_inset: None,
            #[cfg(any(feature = "inspector", debug_assertions))]
            inspector: None,
        })
    }

    pub(crate) fn new_focus_listener(
        &self,
        value: AnyWindowFocusListener,
    ) -> (Subscription, impl FnOnce() + use<>) {
        self.interaction.focus_listeners.insert((), value)
    }
}

impl Window {
    fn mark_view_dirty(&mut self, view_id: EntityId) {
        // Mark ancestor views as dirty. If already in the `dirty_views` set, then all its ancestors
        // should already be dirty.
        for view_id in self
            .interaction
            .rendered_frame
            .dispatch_tree
            .view_path_reversed(view_id)
        {
            if !self.frame_scheduler.insert_dirty_view(view_id) {
                break;
            }
        }
    }

    pub(crate) fn is_view_dirty(&self, view_id: EntityId) -> bool {
        self.frame_scheduler.is_view_dirty(view_id)
    }

    pub(crate) fn is_refreshing(&self) -> bool {
        self.frame_scheduler.is_refreshing()
    }

    pub(crate) fn replace_refreshing(&mut self, refreshing: bool) -> bool {
        self.frame_scheduler.replace_refreshing(refreshing)
    }

    pub(crate) fn force_refresh(&mut self) {
        self.frame_scheduler.set_refreshing(true);
        self.invalidator.set_dirty(true);
    }

    /// Registers a callback to be invoked when the window appearance changes.
    pub fn observe_window_appearance(
        &self,
        mut callback: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let (subscription, activate) = self.appearance_observers.insert(
            (),
            Box::new(move |window, cx| {
                callback(window, cx);
                true
            }),
        );
        activate();
        subscription
    }

    /// Registers a callback to be invoked when the window button layout changes.
    pub fn observe_button_layout_changed(
        &self,
        mut callback: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let (subscription, activate) = self.button_layout_observers.insert(
            (),
            Box::new(move |window, cx| {
                callback(window, cx);
                true
            }),
        );
        activate();
        subscription
    }

    /// Replaces the root entity of the window with a new one.
    pub fn replace_root<E>(
        &mut self,
        cx: &mut App,
        build_view: impl FnOnce(&mut Window, &mut Context<E>) -> E,
    ) -> Entity<E>
    where
        E: 'static + Render,
    {
        let view = cx.new(|cx| build_view(self, cx));
        self.root = Some(view.clone().into());
        self.refresh();
        view
    }

    /// Returns the root entity of the window, if it has one.
    pub fn root<E>(&self) -> Option<Option<Entity<E>>>
    where
        E: 'static + Render,
    {
        self.root
            .as_ref()
            .map(|view| view.clone().downcast::<E>().ok())
    }

    /// Obtain a handle to the window that belongs to this context.
    pub fn window_handle(&self) -> AnyWindowHandle {
        self.handle
    }

    /// Mark the window as dirty, scheduling it to be redrawn on the next frame.
    pub fn refresh(&mut self) {
        if self.invalidator.not_drawing() {
            self.force_refresh();
        }
    }

    /// Close this window.
    pub fn remove_window(&mut self) {
        self.removed = true;
    }

    /// Obtain the currently focused [`FocusHandle`]. If no elements are focused, returns `None`.
    pub fn focused(&self, cx: &App) -> Option<FocusHandle> {
        self.interaction.focused(cx)
    }

    /// Move focus to the element associated with the given [`FocusHandle`].
    pub fn focus(&mut self, handle: &FocusHandle, cx: &mut App) {
        InteractionOwner::focus(self, handle, cx);
    }

    /// Remove focus from all elements within this context's window.
    pub fn blur(&mut self, cx: &mut App) {
        InteractionOwner::blur(self, cx);
    }

    /// Blur the window and don't allow anything in it to be focused again.
    pub fn disable_focus(&mut self, cx: &mut App) {
        InteractionOwner::disable_focus(self, cx);
    }

    /// Move focus to next tab stop.
    pub fn focus_next(&mut self, cx: &mut App) {
        InteractionOwner::focus_next(self, cx);
    }

    /// Move focus to previous tab stop.
    pub fn focus_prev(&mut self, cx: &mut App) {
        InteractionOwner::focus_prev(self, cx);
    }

    /// Accessor for the text system.
    pub fn text_system(&self) -> &Rc<WindowTextSystem> {
        &self.text_system
    }

    /// The current text style. Which is composed of all the style refinements provided to `with_text_style`.
    pub fn text_style(&self) -> TextStyle {
        let mut style = TextStyle::default();
        for refinement in self.frame_builder.text_styles() {
            style.refine(refinement);
        }
        style
    }

    /// Check if the platform window is maximized.
    ///
    /// On some platforms (namely Windows) this is different than the bounds being the size of the display
    pub fn is_maximized(&self) -> bool {
        self.platform_window.is_maximized()
    }

    /// request a certain window decoration (Wayland)
    pub fn request_decorations(&self, decorations: WindowDecorations) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .decorations
        {
            self.platform_window.request_decorations(decorations);
        }
    }

    /// Start a window resize operation if this window is resizable.
    pub fn start_window_resize(&self, edge: ResizeEdge) {
        if self.is_resizable
            && self
                .platform_window
                .capabilities()
                .window_controls
                .resize_window
        {
            self.platform_window.start_window_resize(edge);
        }
    }

    /// Linux (wayland) only: Set the window's input region, the area that receives pointer
    /// and touch input. Events outside it pass through to whatever is below the window.
    ///
    /// - `Some(rects)` restricts input to the union of `rects`, in window coordinates.
    /// - `Some(&[])` is an empty region, so the window receives no pointer or touch input.
    /// - `None` resets the region to the default, so the whole window receives input again.
    pub fn set_input_region(&self, region: Option<&[Bounds<Pixels>]>) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .input_region
        {
            self.platform_window.set_input_region(region);
        }
    }

    /// Return the `WindowBounds` to indicate that how a window should be opened
    /// after it has been closed
    pub fn window_bounds(&self) -> WindowBounds {
        self.platform_window.window_bounds()
    }

    /// Returns the capabilities declared by this window's platform backend.
    pub fn platform_capabilities(&self) -> PlatformCapabilities {
        self.platform_window.capabilities()
    }

    /// Dispatches one accessibility action against the completed frame.
    #[cfg(feature = "accessibility")]
    pub fn dispatch_accessibility_action(
        &mut self,
        node_id: accesskit::NodeId,
        action: accesskit::Action,
        data: Option<&accesskit::ActionData>,
        cx: &mut App,
    ) -> bool {
        let mut actions = mem::take(&mut self.interaction.rendered_frame.accessibility_actions);
        let handled = actions.dispatch(node_id, action, data, self, cx);
        self.interaction.rendered_frame.accessibility_actions = actions;
        handled
    }

    /// Connects a semantic node to GPUI keyboard focus.
    #[cfg(feature = "accessibility")]
    #[doc(hidden)]
    pub fn register_accessibility_focus(
        &mut self,
        node_id: accesskit::NodeId,
        focus_handle: FocusHandle,
    ) {
        if focus_handle.is_focused(self)
            && let Err(error) = self
                .interaction
                .next_frame
                .accessibility_builder
                .set_focus(node_id)
        {
            log::error!("failed to map semantic focus: {error:?}");
        }
        self.interaction.next_frame.accessibility_actions.register(
            node_id,
            accesskit::Action::Focus,
            move |_, window, cx| {
                focus_handle.focus(window, cx);
            },
        );
    }

    /// Returns the completed semantic snapshot for integration tests.
    #[cfg(all(feature = "accessibility", any(test, feature = "test-support")))]
    pub fn accessibility_snapshot_for_test(
        &self,
    ) -> Option<crate::accessibility::SemanticSnapshot> {
        self.interaction
            .built_frame(&self.text_input)
            .accessibility
            .semantic_snapshot()
            .cloned()
    }

    /// Return the `WindowBounds` excluding insets (Wayland and X11)
    pub fn inner_window_bounds(&self) -> WindowBounds {
        self.platform_window.inner_window_bounds()
    }

    /// Dispatch the given action on the currently focused element.
    pub fn dispatch_action(&mut self, action: Box<dyn Action>, cx: &mut App) {
        let focus_id = self.focused(cx).map(|handle| handle.id);

        let window = self.handle;
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    let node_id = window.focus_node_id_in_rendered_frame(focus_id);
                    window.dispatch_action_on_node(node_id, action.as_ref(), cx);
                })
                .log_err();
        })
    }

    pub(crate) fn dispatch_keystroke_observers(
        &mut self,
        keystroke: &Keystroke,
        input_preference: InputPreference,
        action: Option<&dyn Action>,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        cx.keystroke_observers.clone().retain(&(), move |callback| {
            (callback)(
                &KeystrokeEvent {
                    keystroke: keystroke.clone(),
                    input_preference,
                    action: action.map(|action| action.boxed_clone()),
                    context_stack: context_stack.clone(),
                },
                self,
                cx,
            )
        });
    }

    pub(crate) fn dispatch_keystroke_interceptors(
        &mut self,
        keystroke: &Keystroke,
        input_preference: InputPreference,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        cx.keystroke_interceptors
            .clone()
            .retain(&(), move |callback| {
                (callback)(
                    &KeystrokeEvent {
                        keystroke: keystroke.clone(),
                        input_preference,
                        action: None,
                        context_stack: context_stack.clone(),
                    },
                    self,
                    cx,
                )
            });
    }

    /// Schedules the given function to be run at the end of the current effect cycle, allowing entities
    /// that are currently on the stack to be returned to the app.
    pub fn defer(&self, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
        let handle = self.handle;
        cx.defer(move |cx| {
            handle.update(cx, |_, window, cx| f(window, cx)).ok();
        });
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn observe<T: 'static>(
        &mut self,
        observed: &Entity<T>,
        cx: &mut App,
        mut on_notify: impl FnMut(Entity<T>, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let entity_id = observed.entity_id();
        let observed = observed.downgrade();
        let window_handle = self.handle;
        cx.new_observer(
            entity_id,
            Box::new(move |cx| {
                window_handle
                    .update(cx, |_, window, cx| {
                        if let Some(handle) = observed.upgrade() {
                            on_notify(handle, window, cx);
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            }),
        )
    }

    /// Subscribe to events emitted by a entity.
    /// The entity to which you're subscribing must implement the [`EventEmitter`] trait.
    /// The callback will be invoked a handle to the emitting entity, the event, and a window context for the current window.
    pub fn subscribe<Emitter, Evt>(
        &mut self,
        entity: &Entity<Emitter>,
        cx: &mut App,
        mut on_event: impl FnMut(Entity<Emitter>, &Evt, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        Emitter: EventEmitter<Evt>,
        Evt: 'static,
    {
        let entity_id = entity.entity_id();
        let handle = entity.downgrade();
        let window_handle = self.handle;
        cx.new_subscription(
            entity_id,
            (
                TypeId::of::<Evt>(),
                Box::new(move |event, cx| {
                    window_handle
                        .update(cx, |_, window, cx| {
                            if let Some(entity) = handle.upgrade() {
                                let event = event.downcast_ref().expect("invalid event type");
                                on_event(entity, event, window, cx);
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false)
                }),
            ),
        )
    }

    /// Register a callback to be invoked when the given `Entity` is released.
    pub fn observe_release<T>(
        &self,
        entity: &Entity<T>,
        cx: &mut App,
        mut on_release: impl FnOnce(&mut T, &mut Window, &mut App) + 'static,
    ) -> Subscription
    where
        T: 'static,
    {
        let entity_id = entity.entity_id();
        let window_handle = self.handle;
        let (subscription, activate) = cx.release_listeners.insert(
            entity_id,
            Box::new(move |entity, cx| {
                let entity = entity.downcast_mut().expect("invalid entity type");
                window_handle
                    .update(cx, |_, window, cx| on_release(entity, window, cx))
                    .log_err();
            }),
        );
        activate();
        subscription
    }

    /// Creates an [`AsyncWindowContext`], which has a static lifetime and can be held across
    /// await points in async code.
    pub fn to_async(&self, cx: &App) -> AsyncWindowContext {
        AsyncWindowContext::new_context(cx.to_async(), self.handle)
    }

    /// Schedule the given closure to be run directly after the current frame is rendered.
    pub fn on_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        self.frame_scheduler.queue_next_frame(Box::new(callback));
        self.invalidator.wake_platform();
    }

    /// Schedule a frame to be drawn on the next animation frame.
    ///
    /// This is useful for elements that need to animate continuously, such as a video player or an animated GIF.
    /// It will cause the window to redraw on the next frame, even if no other changes have occurred.
    ///
    /// If called from within a view, it will notify that view on the next frame. Otherwise, it will refresh the entire window.
    pub fn request_animation_frame(&self) {
        let entity = self.current_view();
        self.on_next_frame(move |_, cx| cx.notify(entity));
    }

    /// Spawn the future returned by the given closure on the application thread pool.
    /// The closure is provided a handle to the current window and an `AsyncWindowContext` for
    /// use within your future.
    #[track_caller]
    pub fn spawn<AsyncFn, R>(&self, cx: &App, f: AsyncFn) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn(async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }

    /// Spawn the future returned by the given closure on the application thread
    /// pool, with the given priority. The closure is provided a handle to the
    /// current window and an `AsyncWindowContext` for use within your future.
    #[track_caller]
    pub fn spawn_with_priority<AsyncFn, R>(
        &self,
        priority: Priority,
        cx: &App,
        f: AsyncFn,
    ) -> Task<R>
    where
        R: 'static,
        AsyncFn: AsyncFnOnce(&mut AsyncWindowContext) -> R + 'static,
    {
        let handle = self.handle;
        cx.spawn_with_priority(priority, async move |app| {
            let mut async_window_cx = AsyncWindowContext::new_context(app.clone(), handle);
            f(&mut async_window_cx).await
        })
    }

    /// Notify the window that its bounds have changed.
    ///
    /// This updates internal state like `viewport_size` and `scale_factor` from
    /// the platform window, then notifies observers. Normally called automatically
    /// by the platform's resize callback, but exposed publicly for test infrastructure.
    pub fn bounds_changed(&mut self, cx: &mut App) {
        self.scale_factor = self.platform_window.scale_factor();
        self.viewport_size = self.platform_window.content_size();
        self.display_id = self.platform_window.display().map(|display| display.id());
        self.interaction.mouse_position = self.platform_window.mouse_position();
        if self.platform_window.capabilities().accessibility {
            self.platform_window
                .update_accessibility_window_bounds()
                .log_err();
        }

        self.refresh();

        self.bounds_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    /// Returns the bounds of the current window in the global coordinate space, which could span across multiple displays.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.platform_window.bounds()
    }

    /// Renders the current frame's scene to a texture and returns the pixel data as an RGBA image.
    /// This does not present the frame to screen - useful for visual testing where we want
    /// to capture what would be rendered without displaying it or requiring the window to be visible.
    #[cfg(any(test, feature = "test-support"))]
    pub fn render_to_image(&self) -> anyhow::Result<image::RgbaImage> {
        let built_frame = self.interaction.built_frame(&self.text_input);
        self.platform_window.render_to_image(built_frame.scene)
    }

    /// Set the content size of the window.
    pub fn resize(&mut self, size: Size<Pixels>) {
        self.platform_window.resize(size);
    }

    /// Returns whether or not the window is currently fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.platform_window.is_fullscreen()
    }

    /// Returns whether the window is currently in simple (borderless) fullscreen,
    /// where it covers the entire screen including the menu bar and notch area.
    /// Always `false` on platforms other than macOS.
    pub fn is_simple_fullscreen(&self) -> bool {
        self.platform_window
            .capabilities()
            .window_services
            .simple_fullscreen
            && self.platform_window.is_simple_fullscreen()
    }

    pub(crate) fn appearance_changed(&mut self, cx: &mut App) {
        self.appearance = self.platform_window.appearance();

        self.appearance_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    pub(crate) fn button_layout_changed(&mut self, cx: &mut App) {
        self.button_layout_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    /// Returns the appearance of the current window.
    pub fn appearance(&self) -> WindowAppearance {
        self.appearance
    }

    /// Returns the size of the drawable area within the window.
    pub fn viewport_size(&self) -> Size<Pixels> {
        self.viewport_size
    }

    /// Returns whether this window is focused by the operating system (receiving key events).
    pub fn is_window_active(&self) -> bool {
        self.active.get()
    }

    /// Returns whether this window is considered to be the window
    /// that currently owns the mouse cursor.
    /// On mac, this is equivalent to `is_window_active`.
    pub fn is_window_hovered(&self) -> bool {
        if cfg!(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )) {
            self.hovered.get()
        } else {
            self.is_window_active()
        }
    }

    /// Toggle zoom on the window.
    ///
    /// Returns an error when the platform backend cannot zoom the window.
    pub fn zoom_window(&self) -> Result<()> {
        self.platform_window.zoom()
    }

    /// Opens the native title bar context menu, useful when implementing client side decorations (Wayland and X11)
    pub fn show_window_menu(&self, position: Point<Pixels>) {
        if self
            .platform_window
            .capabilities()
            .window_controls
            .window_menu
        {
            self.platform_window.show_window_menu(position)
        }
    }

    /// Handle window movement for Linux and macOS.
    /// Tells the compositor to take control of window movement (Wayland and X11)
    ///
    /// Events may not be received during a move operation.
    pub fn start_window_move(&self) {
        if self
            .platform_window
            .capabilities()
            .window_controls
            .move_window
        {
            self.platform_window.start_window_move()
        }
    }

    /// When using client side decorations, set this to the width of the invisible decorations (Wayland and X11)
    pub fn set_client_inset(&mut self, inset: Pixels) {
        self.client_inset = Some(inset);
        if self
            .platform_window
            .capabilities()
            .window_services
            .client_inset
        {
            self.platform_window.set_client_inset(inset);
        }
    }

    /// Sets the client-drawn corner radii so platform effects can match the window's shape.
    pub fn set_client_corner_radii(&mut self, radii: Corners<Pixels>) {
        self.platform_window.set_client_corner_radii(radii);
    }

    /// Returns the client_inset value by [`Self::set_client_inset`].
    pub fn client_inset(&self) -> Option<Pixels> {
        self.client_inset
    }

    /// Returns whether the title bar window controls need to be rendered by the application (Wayland and X11)
    pub fn window_decorations(&self) -> Decorations {
        self.platform_window.window_decorations()
    }

    /// Returns whether this window is resizable.
    pub fn is_resizable(&self) -> bool {
        self.is_resizable
    }

    /// Returns whether this window is minimizable.
    pub fn is_minimizable(&self) -> bool {
        self.is_minimizable
    }

    /// Returns the controls supported by the platform.
    pub fn window_controls(&self) -> WindowControls {
        self.platform_window.window_controls()
    }

    /// Updates the window's title at the platform level.
    pub fn set_window_title(&mut self, title: &str) {
        self.platform_window.set_title(title);
    }

    /// Sets the position of the macOS traffic light buttons.
    #[cfg(target_os = "macos")]
    pub fn set_traffic_light_position(&self, position: Point<Pixels>) {
        self.platform_window.set_traffic_light_position(position);
    }

    /// Sets the application identifier.
    pub fn set_app_id(&mut self, app_id: &str) {
        if self.platform_window.capabilities().window_services.app_id {
            self.platform_window.set_app_id(app_id);
        }
    }

    /// Sets the window background appearance.
    pub fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.platform_window
            .set_background_appearance(background_appearance);
    }

    /// Mark the window as dirty at the platform level.
    pub fn set_window_edited(&mut self, edited: bool) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .document_metadata
        {
            self.platform_window.set_edited(edited);
        }
    }

    /// Set the path of the file this window represents.
    /// On macOS, this sets the window's accessibility document property (AXDocument).
    pub fn set_document_path(&self, path: Option<&std::path::Path>) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .document_metadata
        {
            self.platform_window.set_document_path(path);
        }
    }

    /// Determine the display on which the window is visible.
    pub fn display(&self, cx: &App) -> Option<Rc<dyn PlatformDisplay>> {
        cx.platform
            .displays()
            .into_iter()
            .find(|display| Some(display.id()) == self.display_id)
    }

    /// Show the platform character palette.
    pub fn show_character_palette(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .character_palette
        {
            self.platform_window.show_character_palette();
        }
    }

    /// The scale factor of the display associated with the window. For example, it could
    /// return 2.0 for a "retina" display, indicating that each logical pixel should actually
    /// be rendered as two pixels on screen.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// The size of an em for the base font of the application. Adjusting this value allows the
    /// UI to scale, just like zooming a web page.
    pub fn rem_size(&self) -> Pixels {
        self.rem_size_override_stack
            .last()
            .copied()
            .unwrap_or(self.rem_size)
    }

    /// Sets the size of an em for the base font of the application. Adjusting this value allows the
    /// UI to scale, just like zooming a web page.
    pub fn set_rem_size(&mut self, rem_size: impl Into<Pixels>) {
        self.rem_size = rem_size.into();
    }

    /// Acquire a globally unique identifier for the given ElementId.
    /// Only valid for the duration of the provided closure.
    pub fn with_global_id<R>(
        &mut self,
        element_id: ElementId,
        f: impl FnOnce(&GlobalElementId, &mut Self) -> R,
    ) -> R {
        self.with_id(element_id, |this| {
            let global_id = this.global_element_id();

            f(&global_id, this)
        })
    }

    /// Calls the provided closure with the element ID pushed on the stack.
    #[inline]
    pub fn with_id<R>(
        &mut self,
        element_id: impl Into<ElementId>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.push_element_id(element_id.into());
        let result = f(self);
        self.pop_element_id();
        result
    }

    pub(crate) fn push_element_id(&mut self, element_id: ElementId) {
        self.frame_builder.push_element_id(element_id);
    }

    pub(crate) fn pop_element_id(&mut self) {
        self.frame_builder.pop_element_id();
    }

    pub(crate) fn element_id_path(&self) -> &[ElementId] {
        self.frame_builder.element_id_path()
    }

    pub(crate) fn global_element_id(&self) -> GlobalElementId {
        self.frame_builder.global_element_id()
    }

    /// Executes the provided function with the specified rem size.
    ///
    /// This method must only be called as part of element drawing.
    // This function is called in a highly recursive manner in editor
    // prepainting, make sure its inlined to reduce the stack burden
    #[inline]
    pub fn with_rem_size<F, R>(&mut self, rem_size: Option<impl Into<Pixels>>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.invalidator.debug_assert_paint_or_prepaint();

        if let Some(rem_size) = rem_size {
            self.rem_size_override_stack.push(rem_size.into());
            let result = f(self);
            self.rem_size_override_stack.pop();
            result
        } else {
            f(self)
        }
    }

    /// The line height associated with the current text style.
    pub fn line_height(&self) -> Pixels {
        self.text_style().line_height_in_pixels(self.rem_size())
    }

    /// Rounds a logical value to the nearest device pixel.
    #[inline]
    pub fn pixel_snap(&self, value: Pixels) -> Pixels {
        px(round_to_device_pixel(value.0, self.scale_factor()) / self.scale_factor())
    }

    /// f64 variant of [`Self::pixel_snap`].
    #[inline]
    pub fn pixel_snap_f64(&self, value: f64) -> f64 {
        let scale_factor = f64::from(self.scale_factor());
        round_half_toward_zero_f64(value * scale_factor) / scale_factor
    }

    /// Snaps a bounds' origin and size to the nearest device pixel.
    #[inline]
    pub fn pixel_snap_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        bounds.map(|c| self.pixel_snap(c))
    }

    /// Snaps a point's coordinates to the nearest device pixel.
    #[inline]
    pub fn pixel_snap_point(&self, position: Point<Pixels>) -> Point<Pixels> {
        position.map(|c| self.pixel_snap(c))
    }

    #[inline]
    pub(super) fn snap_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<ScaledPixels> {
        let scale_factor = self.scale_factor();
        let left = round_to_device_pixel(bounds.left().0, scale_factor);
        let top = round_to_device_pixel(bounds.top().0, scale_factor);
        let right = round_to_device_pixel(bounds.right().0, scale_factor).max(left);
        let bottom = round_to_device_pixel(bounds.bottom().0, scale_factor).max(top);
        Bounds::from_corners(
            point(ScaledPixels(left), ScaledPixels(top)),
            point(ScaledPixels(right), ScaledPixels(bottom)),
        )
    }

    /// Rounds half-to-zero but clamps any non-zero input up to 1 dp so thin strokes do not disappear.
    #[inline]
    pub(super) fn snap_stroke(&self, value: Pixels) -> ScaledPixels {
        ScaledPixels(round_stroke_to_device_pixel(value.0, self.scale_factor()))
    }

    #[inline]
    pub(super) fn snap_border_widths(&self, edges: Edges<Pixels>) -> Edges<ScaledPixels> {
        edges.map(|e| self.snap_stroke(*e))
    }

    /// Floors the near edge and ceils the far edge, producing a strict superset of the raw region.
    #[inline]
    pub(super) fn cover_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<ScaledPixels> {
        let scale_factor = self.scale_factor();
        let left = floor_to_device_pixel(bounds.left().0, scale_factor);
        let top = floor_to_device_pixel(bounds.top().0, scale_factor);
        let right = ceil_to_device_pixel(bounds.right().0, scale_factor).max(left);
        let bottom = ceil_to_device_pixel(bounds.bottom().0, scale_factor).max(top);
        Bounds::from_corners(
            point(ScaledPixels(left), ScaledPixels(top)),
            point(ScaledPixels(right), ScaledPixels(bottom)),
        )
    }

    #[inline]
    pub(super) fn snapped_content_mask(&self) -> ContentMask<ScaledPixels> {
        ContentMask {
            bounds: self.cover_bounds(self.content_mask().bounds),
        }
    }

    /// Call to prevent the default action of an event. Currently only used to prevent
    /// parent elements from becoming focused on mouse down.
    pub fn prevent_default(&mut self) {
        self.interaction.default_prevented = true;
    }

    /// Obtain whether default has been prevented for the event currently being dispatched.
    pub fn default_prevented(&self) -> bool {
        self.interaction.default_prevented
    }

    /// Determine whether the given action is available along the dispatch path to the currently focused element.
    pub fn is_action_available(&self, action: &dyn Action, cx: &App) -> bool {
        let node_id =
            self.focus_node_id_in_rendered_frame(self.focused(cx).map(|handle| handle.id));
        self.interaction
            .rendered_frame
            .dispatch_tree
            .is_action_available(action, node_id)
    }

    /// Determine whether the given action is available along the dispatch path to the given focus_handle.
    pub fn is_action_available_in(&self, action: &dyn Action, focus_handle: &FocusHandle) -> bool {
        let node_id = self.focus_node_id_in_rendered_frame(Some(focus_handle.id));
        self.interaction
            .rendered_frame
            .dispatch_tree
            .is_action_available(action, node_id)
    }

    /// The position of the mouse relative to the window.
    pub fn mouse_position(&self) -> Point<Pixels> {
        self.interaction.mouse_position
    }

    /// Captures the pointer for the given hitbox. While captured, all mouse move and mouse up
    /// events will be routed to listeners that check this hitbox's `is_hovered` status,
    /// regardless of actual hit testing. This enables drag operations that continue
    /// even when the pointer moves outside the element's bounds.
    ///
    /// The capture is automatically released on mouse up.
    pub fn capture_pointer(&mut self, hitbox_id: HitboxId) {
        self.interaction.captured_hitbox = Some(hitbox_id);
    }

    /// Releases any active pointer capture.
    pub fn release_pointer(&mut self) {
        self.interaction.captured_hitbox = None;
    }

    /// Returns the hitbox that has captured the pointer, if any.
    pub fn captured_hitbox(&self) -> Option<HitboxId> {
        self.interaction.captured_hitbox
    }

    /// The current state of the keyboard's modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.interaction.modifiers
    }

    /// Returns true if the last input event was keyboard-based (key press, tab navigation, etc.)
    /// This is used for focus-visible styling to show focus indicators only for keyboard navigation.
    pub fn last_input_was_keyboard(&self) -> bool {
        self.interaction.last_input_modality == InputModality::Keyboard
    }

    /// The current state of the keyboard's capslock
    pub fn capslock(&self) -> Capslock {
        self.interaction.capslock
    }

    fn complete_frame(&self) {
        self.platform_window.completed_frame();
    }

    /// Produces a new frame and assigns it to `rendered_frame`. To actually show
    /// the contents of the new [`Scene`], use [`Self::present`].
    #[profiling::function]
    pub fn draw(&mut self, cx: &mut App) -> ArenaClearNeeded {
        let atlas_frame = self.sprite_atlas.begin_frame();
        if self.interaction.rendered_frame.atlas_epoch != atlas_frame.epoch() {
            self.force_refresh();
        }
        #[cfg(feature = "frame-diagnostics")]
        let draw_start = Instant::now();
        #[cfg(feature = "frame-diagnostics")]
        self.invalidator.begin_frame(draw_start);
        // Set up the per-App arena for element allocation during this draw.
        // This ensures that multiple test Apps have isolated arenas.
        let _arena_scope = ElementArenaScope::enter(&cx.element_arena);

        self.invalidate_entities();
        cx.entities.clear_accessed();
        self.frame_builder.debug_assert_idle();
        self.invalidator.set_dirty(false);
        self.frame_builder.reset_autoscroll();

        // Restore the previously-used input handler.
        // Place it back into a None slot (left by a previous .take()) so that
        // cached paint_range indices in reuse_paint find the handler at the
        // expected position.
        if let Some(input_handler) = self.platform_window.take_input_handler() {
            self.text_input.restore_rendered_handler(input_handler);
        }
        if !cx.mode.skip_drawing() {
            self.draw_roots(cx);
        }
        self.frame_scheduler.clear_dirty_views();
        self.interaction.next_frame.window_active = self.active.get();

        // Register requested input handler with the platform window.
        // Use .take() instead of .pop() to preserve Vec length, so that cached
        // paint_range indices remain valid for reuse_paint on the next frame.
        // Search backwards to find the last Some entry, since reuse_paint may
        // have copied None slots from the previous frame. (Fixes #50456)
        if let Some(input_handler) = self
            .text_input
            .next_handlers
            .iter_mut()
            .rev()
            .find_map(|h| h.take())
        {
            self.platform_window.set_input_handler(input_handler);
        }

        self.frame_builder.layout_engine().clear();
        self.text_system().finish_frame();
        self.interaction
            .next_frame
            .finish(&mut self.interaction.rendered_frame);
        self.interaction.next_frame.atlas_epoch = self
            .sprite_atlas
            .finish_frame(atlas_frame, self.interaction.next_frame.scene.atlas_usage());

        self.invalidator.set_phase(DrawPhase::Focus);
        let previous_focus_path = self.interaction.rendered_frame.focus_path();
        let previous_window_active = self.interaction.rendered_frame.window_active;
        mem::swap(
            &mut self.interaction.rendered_frame,
            &mut self.interaction.next_frame,
        );
        mem::swap(
            &mut self.text_input.rendered_handlers,
            &mut self.text_input.next_handlers,
        );
        self.interaction.next_frame.clear();
        self.text_input.next_handlers.clear();
        let current_focus_path = self.interaction.rendered_frame.focus_path();
        let current_window_active = self.interaction.rendered_frame.window_active;
        let mut focus_before_listeners = self.interaction.focus;

        if previous_focus_path != current_focus_path
            || previous_window_active != current_window_active
        {
            if !previous_focus_path.is_empty() && current_focus_path.is_empty() {
                self.interaction
                    .focus_lost_listeners
                    .clone()
                    .retain(&(), |listener| listener(self, cx));
                // Focus-lost fallbacks may move focus to a node outside the current tree.
                // Track only focus changes made by listeners to avoid a redraw loop.
                focus_before_listeners = self.interaction.focus;
            }

            let event = WindowFocusEvent {
                previous_focus_path: if previous_window_active {
                    previous_focus_path
                } else {
                    Default::default()
                },
                current_focus_path: if current_window_active {
                    current_focus_path
                } else {
                    Default::default()
                },
            };
            self.interaction
                .focus_listeners
                .clone()
                .retain(&(), |listener| listener(&event, self, cx));
        }

        self.frame_builder.debug_assert_idle();
        self.record_entities_accessed(cx);
        self.reset_cursor_style(cx);
        self.frame_scheduler.set_refreshing(false);
        self.invalidator.set_phase(DrawPhase::None);
        // Focus movement during listener dispatch is suppressed while drawing. Redraw so the
        // new focus state and its corresponding events are delivered on the next frame.
        if self.interaction.focus != focus_before_listeners {
            self.refresh();
        }
        self.frame_scheduler.mark_present_pending();

        #[cfg(feature = "frame-diagnostics")]
        if let Some(frame) = self.invalidator.finish_frame() {
            let timing = FrameTiming {
                build_id: frame.build_id,
                window_id: self.handle.window_id(),
                dirty_at: frame.dirty_at,
                invalidations: frame.invalidations,
                draw_start,
                draw_end: Instant::now(),
                input_started_at: frame.input_started_at,
                input: frame.input,
            };
            self.invalidator
                .record_event(FrameEvent::DrawFinished(timing));
            self.interaction.rendered_frame.diagnostics =
                BuiltFrameDiagnosticsSnapshot::from_timing(timing);
        }

        ArenaClearNeeded::new(&cx.element_arena)
    }

    fn record_entities_accessed(&mut self, cx: &mut App) {
        let entities = cx.entities.take_accessed();
        let handle = self.handle;
        cx.record_entities_accessed(
            handle,
            // Try moving window invalidator into the Window
            self.invalidator.clone(),
            &entities,
        );
        cx.entities.restore_accessed(entities);
    }

    fn invalidate_entities(&mut self) {
        let mut views = self.invalidator.take_views();
        for entity in views.drain() {
            self.mark_view_dirty(entity);
        }
        self.invalidator.replace_views(views);
    }

    #[profiling::function]
    fn present(&mut self) {
        #[cfg(feature = "frame-diagnostics")]
        let present_start = Instant::now();
        let built_frame = self.interaction.built_frame(&self.text_input);
        #[cfg(feature = "frame-diagnostics")]
        let frame_timing = built_frame.diagnostics.timing();
        if self.platform_window.capabilities().accessibility {
            self.platform_window
                .update_accessibility(built_frame.accessibility.clone())
                .log_err();
        }
        let submission = crate::render_api::submit_platform_frame(
            self.platform_window.as_mut(),
            crate::render_api::RenderScene::new(built_frame.scene),
        );
        if !submission.submitted {
            log::debug!("platform render target skipped frame submission");
        }
        #[cfg(feature = "frame-diagnostics")]
        if let Some(frame) = frame_timing {
            let present_end = Instant::now();
            let event = if submission.submitted {
                FrameEvent::Presented(FramePresentationTiming {
                    build_id: frame.build_id,
                    window_id: self.handle.window_id(),
                    present_start,
                    present_end,
                    input_to_present: frame
                        .input_started_at
                        .map(|started_at| present_end.duration_since(started_at)),
                    input: frame.input,
                })
            } else {
                FrameEvent::SubmissionSkipped {
                    build_id: frame.build_id,
                    window_id: self.handle.window_id(),
                    at: present_end,
                }
            };
            self.invalidator.record_event(event);
        }
        #[cfg(feature = "input-latency-histogram")]
        if submission.submitted {
            self.input_latency_tracker.record_frame_presented();
        }
        self.frame_scheduler.clear_present_pending();
        #[cfg(feature = "frame-diagnostics")]
        self.invalidator.flush_events();
        profiling::finish_frame!();
    }

    /// Builds and presents one frame for benchmark and integration-test infrastructure.
    #[cfg(any(test, feature = "test-support"))]
    pub fn draw_and_present_for_test(&mut self, cx: &mut App) {
        self.draw(cx).clear();
        self.present();
    }

    /// Presents the most recently built frame for benchmark and integration-test infrastructure.
    #[cfg(any(test, feature = "test-support"))]
    pub fn present_for_test(&mut self) {
        self.present();
    }

    /// Returns a snapshot of the current input-latency histograms.
    #[cfg(feature = "input-latency-histogram")]
    pub fn input_latency_snapshot(&self) -> InputLatencySnapshot {
        self.input_latency_tracker.snapshot()
    }

    fn draw_roots(&mut self, cx: &mut App) {
        self.invalidator.set_phase(DrawPhase::Prepaint);
        #[cfg(feature = "frame-diagnostics")]
        let prepaint_started_at = Instant::now();
        #[cfg(feature = "frame-diagnostics")]
        let mut request_layout_duration = Duration::ZERO;
        #[cfg(feature = "frame-diagnostics")]
        let mut request_layout_operations = 0;
        self.interaction.tooltip_bounds.take();

        let _inspector_width: Pixels = rems(30.0).to_pixels(self.rem_size());
        let root_size = {
            #[cfg(any(feature = "inspector", debug_assertions))]
            {
                if self.inspector.is_some() {
                    let mut size = self.viewport_size;
                    size.width = (size.width - _inspector_width).max(px(0.0));
                    size
                } else {
                    self.viewport_size
                }
            }
            #[cfg(not(any(feature = "inspector", debug_assertions)))]
            {
                self.viewport_size
            }
        };

        // Layout all root elements. Like the root element on the web, which
        // stretches to fill the viewport unless explicitly sized, window roots
        // fill the window when their size is `auto`.
        let scale_factor = self.scale_factor();
        let mut root_element = self
            .root
            .as_ref()
            .expect("value should have the expected type")
            .clone()
            .into_any();
        #[cfg(feature = "frame-diagnostics")]
        let request_layout_started_at = Instant::now();
        let root_layout_id = root_element.request_layout(self, cx);
        #[cfg(feature = "frame-diagnostics")]
        {
            request_layout_duration += request_layout_started_at.elapsed();
            request_layout_operations += 1;
        }
        self.frame_builder
            .layout_engine()
            .stretch_auto_size_to_fill(root_layout_id, root_size, scale_factor);
        root_element.prepaint_as_root(Point::default(), root_size.into(), self, cx);

        #[cfg(any(feature = "inspector", debug_assertions))]
        let inspector_element = self.prepaint_inspector(_inspector_width, cx);

        self.prepaint_deferred_draws(cx);

        let mut prompt_element = None;
        let mut active_drag_element = None;
        let mut tooltip_element = None;
        if let Some(prompt) = self.prompt.take() {
            let mut element = prompt.view.any_view().into_any();
            #[cfg(feature = "frame-diagnostics")]
            let request_layout_started_at = Instant::now();
            let prompt_layout_id = element.request_layout(self, cx);
            #[cfg(feature = "frame-diagnostics")]
            {
                request_layout_duration += request_layout_started_at.elapsed();
                request_layout_operations += 1;
            }
            self.frame_builder
                .layout_engine()
                .stretch_auto_size_to_fill(prompt_layout_id, root_size, scale_factor);
            element.prepaint_as_root(Point::default(), root_size.into(), self, cx);
            prompt_element = Some(element);
            self.prompt = Some(prompt);
        } else if let Some(active_drag) = cx.active_drag.take() {
            let mut element = active_drag.view.clone().into_any();
            let offset = self.mouse_position() - active_drag.cursor_offset;
            element.prepaint_as_root(offset, AvailableSpace::min_size(), self, cx);
            active_drag_element = Some(element);
            cx.active_drag = Some(active_drag);
        } else {
            tooltip_element = self.prepaint_tooltip(cx);
        }

        self.interaction.mouse_hit_test = self
            .interaction
            .next_frame
            .hit_test(self.interaction.mouse_position);

        #[cfg(feature = "frame-diagnostics")]
        {
            self.record_frame_phase(
                FramePhase::RequestLayout,
                request_layout_duration,
                request_layout_operations,
                0,
            );
            self.record_frame_phase(
                FramePhase::Prepaint,
                prepaint_started_at
                    .elapsed()
                    .saturating_sub(request_layout_duration),
                1,
                0,
            );
        }

        // Now actually paint the elements.
        self.invalidator.set_phase(DrawPhase::Paint);
        #[cfg(feature = "frame-diagnostics")]
        let paint_started_at = Instant::now();
        root_element.paint(self, cx);

        #[cfg(any(feature = "inspector", debug_assertions))]
        self.paint_inspector(inspector_element, cx);

        self.paint_deferred_draws(cx);

        if let Some(mut prompt_element) = prompt_element {
            prompt_element.paint(self, cx);
        } else if let Some(mut drag_element) = active_drag_element {
            drag_element.paint(self, cx);
        } else if let Some(mut tooltip_element) = tooltip_element {
            tooltip_element.paint(self, cx);
        }

        #[cfg(any(feature = "inspector", debug_assertions))]
        self.paint_inspector_hitbox(cx);
        #[cfg(feature = "frame-diagnostics")]
        self.record_frame_phase(FramePhase::Paint, paint_started_at.elapsed(), 1, 0);
    }

    #[cfg(feature = "frame-diagnostics")]
    fn record_frame_phase(
        &self,
        phase: FramePhase,
        duration: Duration,
        operations: u64,
        cache_hits: u64,
    ) {
        let Some(frame) = self.invalidator.active_frame() else {
            return;
        };
        self.invalidator
            .record_event(FrameEvent::Phase(FramePhaseTiming {
                build_id: frame.build_id,
                window_id: self.handle.window_id(),
                phase,
                duration,
                operations,
                cache_hits,
            }));
    }

    fn prepaint_tooltip(&mut self, cx: &mut App) -> Option<AnyElement> {
        // Use indexing instead of iteration to avoid borrowing self for the duration of the loop.
        for tooltip_request_index in (0..self.interaction.next_frame.tooltip_requests.len()).rev() {
            let Some(Some(tooltip_request)) = self
                .interaction
                .next_frame
                .tooltip_requests
                .get(tooltip_request_index)
                .cloned()
            else {
                log::error!("Unexpectedly absent TooltipRequest");
                continue;
            };
            let mut element = tooltip_request.tooltip.view.clone().into_any();
            let mouse_position = tooltip_request.tooltip.mouse_position;
            let tooltip_size = element.layout_as_root(AvailableSpace::min_size(), self, cx);

            let mut tooltip_bounds =
                Bounds::new(mouse_position + point(px(1.), px(1.)), tooltip_size);
            let window_bounds = Bounds {
                origin: Point::default(),
                size: self.viewport_size(),
            };

            if tooltip_bounds.right() > window_bounds.right() {
                let new_x = mouse_position.x - tooltip_bounds.size.width - px(1.);
                if new_x >= Pixels::ZERO {
                    tooltip_bounds.origin.x = new_x;
                } else {
                    tooltip_bounds.origin.x = cmp::max(
                        Pixels::ZERO,
                        tooltip_bounds.origin.x - tooltip_bounds.right() - window_bounds.right(),
                    );
                }
            }

            if tooltip_bounds.bottom() > window_bounds.bottom() {
                let new_y = mouse_position.y - tooltip_bounds.size.height - px(1.);
                if new_y >= Pixels::ZERO {
                    tooltip_bounds.origin.y = new_y;
                } else {
                    tooltip_bounds.origin.y = cmp::max(
                        Pixels::ZERO,
                        tooltip_bounds.origin.y - tooltip_bounds.bottom() - window_bounds.bottom(),
                    );
                }
            }

            // It's possible for an element to have an active tooltip while not being painted (e.g.
            // via the `visible_on_hover` method). Since mouse listeners are not active in this
            // case, instead update the tooltip's visibility here.
            let is_visible =
                (tooltip_request.tooltip.check_visible_and_update)(tooltip_bounds, self, cx);
            if !is_visible {
                continue;
            }

            self.with_absolute_element_offset(tooltip_bounds.origin, |window| {
                element.prepaint(window, cx)
            });

            self.interaction.tooltip_bounds = Some(TooltipBounds {
                id: tooltip_request.id,
                bounds: tooltip_bounds,
            });
            return Some(element);
        }
        None
    }

    fn prepaint_deferred_draws(&mut self, cx: &mut App) {
        assert_eq!(self.frame_builder.element_id_depth(), 0);

        // Process deferred draws in multiple rounds to support nesting.
        // Each round processes all current deferred draws, which may push new ones.
        //
        // The draws are processed in place rather than being moved out of
        // `next_frame.deferred_draws`: `prepaint_index` snapshots that vector's
        // length, so any prepaint range recorded during a round (view caches,
        // nested deferred draws) must index the same vector `reuse_prepaint`
        // slices on the next frame. Moving the draws out and re-appending them
        // shifts the indices of nested draws, causing reused subtrees to graft
        // the wrong deferred draws and panic in the dispatch tree.
        let mut round_start = 0;
        let mut depth = 0;
        loop {
            let round_end = self.interaction.next_frame.deferred_draws.len();
            if round_start == round_end {
                break;
            }
            // Limit maximum nesting depth to prevent infinite loops.
            assert!(depth < 10, "Exceeded maximum (10) deferred depth");
            depth += 1;

            // Sort this round by priority.
            let mut traversal_order = (round_start..round_end).collect::<SmallVec<[usize; 8]>>();
            traversal_order
                .sort_by_key(|ix| self.interaction.next_frame.deferred_draws[*ix].priority);

            for deferred_draw_ix in traversal_order {
                let (element, parent_node, current_view, rem_size, absolute_offset, prepaint_range) = {
                    let deferred_draw =
                        &mut self.interaction.next_frame.deferred_draws[deferred_draw_ix];
                    self.frame_builder
                        .restore_element_ids(&deferred_draw.element_id_stack);
                    self.frame_builder
                        .restore_text_styles(&deferred_draw.text_style_stack);
                    (
                        deferred_draw.element.take(),
                        deferred_draw.parent_node,
                        deferred_draw.current_view,
                        deferred_draw.rem_size,
                        deferred_draw.absolute_offset,
                        deferred_draw.prepaint_range.clone(),
                    )
                };
                self.interaction
                    .next_frame
                    .dispatch_tree
                    .set_active_node(parent_node);

                let prepaint_start = self.prepaint_index();
                if let Some(mut element) = element {
                    let ((), accessed_entities) = cx.detect_accessed_entities(|cx| {
                        self.with_rendered_view(current_view, |window| {
                            window.with_rem_size(Some(rem_size), |window| {
                                window.with_absolute_element_offset(absolute_offset, |window| {
                                    element.prepaint(window, cx);
                                });
                            });
                        });
                    });
                    self.interaction.next_frame.deferred_draws[deferred_draw_ix]
                        .accessed_entities = accessed_entities;
                    self.interaction.next_frame.deferred_draws[deferred_draw_ix].element =
                        Some(element);
                } else {
                    cx.entities.extend_accessed(
                        self.interaction.next_frame.deferred_draws[deferred_draw_ix]
                            .accessed_entities
                            .iter()
                            .copied(),
                    );
                    self.reuse_prepaint(prepaint_range);
                }
                let prepaint_end = self.prepaint_index();
                self.interaction.next_frame.deferred_draws[deferred_draw_ix].prepaint_range =
                    prepaint_start..prepaint_end;
            }

            self.frame_builder.clear_element_ids();
            self.frame_builder.clear_text_styles();
            round_start = round_end;
        }
    }

    fn paint_deferred_draws(&mut self, cx: &mut App) {
        assert_eq!(self.frame_builder.element_id_depth(), 0);

        // Paint all deferred draws in priority order.
        // Since prepaint has already processed nested deferreds, we just paint them all.
        if self.interaction.next_frame.deferred_draws.len() == 0 {
            return;
        }

        let traversal_order = self.deferred_draw_traversal_order();
        let mut deferred_draws = mem::take(&mut self.interaction.next_frame.deferred_draws);
        for deferred_draw_ix in traversal_order {
            let mut deferred_draw = &mut deferred_draws[deferred_draw_ix];
            self.frame_builder
                .restore_element_ids(&deferred_draw.element_id_stack);
            self.interaction
                .next_frame
                .dispatch_tree
                .set_active_node(deferred_draw.parent_node);

            let paint_start = self.paint_index();
            let content_mask = deferred_draw.content_mask;
            if let Some(element) = deferred_draw.element.as_mut() {
                let ((), accessed_entities) = cx.detect_accessed_entities(|cx| {
                    self.with_rendered_view(deferred_draw.current_view, |window| {
                        window.with_content_mask(content_mask, |window| {
                            window.with_rem_size(Some(deferred_draw.rem_size), |window| {
                                element.paint(window, cx);
                            });
                        })
                    })
                });
                deferred_draw.accessed_entities.extend(accessed_entities);
            } else {
                self.reuse_paint(deferred_draw.paint_range.clone());
            }
            let paint_end = self.paint_index();
            deferred_draw.paint_range = paint_start..paint_end;
        }
        self.interaction.next_frame.deferred_draws = deferred_draws;
        self.frame_builder.clear_element_ids();
    }

    fn deferred_draw_traversal_order(&mut self) -> SmallVec<[usize; 8]> {
        let deferred_count = self.interaction.next_frame.deferred_draws.len();
        let mut sorted_indices = (0..deferred_count).collect::<SmallVec<[_; 8]>>();
        sorted_indices.sort_by_key(|ix| self.interaction.next_frame.deferred_draws[*ix].priority);
        sorted_indices
    }

    pub(crate) fn prepaint_index(&self) -> PrepaintStateIndex {
        PrepaintStateIndex {
            hitboxes_index: self.interaction.next_frame.hitboxes.len(),
            tooltips_index: self.interaction.next_frame.tooltip_requests.len(),
            deferred_draws_index: self.interaction.next_frame.deferred_draws.len(),
            dispatch_tree_index: self.interaction.next_frame.dispatch_tree.len(),
            accessed_element_states_index: self
                .interaction
                .next_frame
                .accessed_element_states
                .len(),
            line_layout_index: self.text_system.layout_index(),
        }
    }

    pub(crate) fn reuse_prepaint(&mut self, range: Range<PrepaintStateIndex>) {
        #[cfg(feature = "frame-diagnostics")]
        let replay_started_at = Instant::now();
        self.interaction.next_frame.hitboxes.extend(
            self.interaction.rendered_frame.hitboxes
                [range.start.hitboxes_index..range.end.hitboxes_index]
                .iter()
                .cloned(),
        );
        self.interaction.next_frame.tooltip_requests.extend(
            self.interaction.rendered_frame.tooltip_requests
                [range.start.tooltips_index..range.end.tooltips_index]
                .iter_mut()
                .map(|request| request.take()),
        );
        self.interaction.next_frame.accessed_element_states.extend(
            self.interaction.rendered_frame.accessed_element_states[range
                .start
                .accessed_element_states_index
                ..range.end.accessed_element_states_index]
                .iter()
                .map(|(id, type_id)| (id.clone(), *type_id)),
        );
        self.text_system
            .reuse_layouts(range.start.line_layout_index..range.end.line_layout_index);

        let reused_subtree = self.interaction.next_frame.dispatch_tree.reuse_subtree(
            range.start.dispatch_tree_index..range.end.dispatch_tree_index,
            &mut self.interaction.rendered_frame.dispatch_tree,
            self.interaction.focus,
        );

        if reused_subtree.contains_focus() {
            self.interaction.next_frame.focus = self.interaction.focus;
        }

        self.interaction.next_frame.deferred_draws.extend(
            self.interaction.rendered_frame.deferred_draws
                [range.start.deferred_draws_index..range.end.deferred_draws_index]
                .iter()
                .map(|deferred_draw| DeferredDraw {
                    current_view: deferred_draw.current_view,
                    parent_node: reused_subtree.refresh_node_id(deferred_draw.parent_node),
                    element_id_stack: deferred_draw.element_id_stack.clone(),
                    text_style_stack: deferred_draw.text_style_stack.clone(),
                    content_mask: deferred_draw.content_mask,
                    rem_size: deferred_draw.rem_size,
                    priority: deferred_draw.priority,
                    element: None,
                    absolute_offset: deferred_draw.absolute_offset,
                    prepaint_range: deferred_draw.prepaint_range.clone(),
                    paint_range: deferred_draw.paint_range.clone(),
                    accessed_entities: deferred_draw.accessed_entities.clone(),
                }),
        );
        #[cfg(feature = "frame-diagnostics")]
        self.record_frame_phase(
            FramePhase::PrepaintCacheReplay,
            replay_started_at.elapsed(),
            1,
            1,
        );
    }

    pub(crate) fn paint_index(&self) -> PaintIndex {
        PaintIndex {
            scene_index: self.interaction.next_frame.scene.len(),
            #[cfg(any(test, feature = "test-support"))]
            debug_bounds_index: self.interaction.next_frame.debug_bounds_records.len(),
            mouse_listeners_index: self.interaction.next_frame.mouse_listeners.len(),
            input_handlers_index: self.text_input.next_handlers.len(),
            cursor_styles_index: self.interaction.next_frame.cursor_styles.len(),
            accessed_element_states_index: self
                .interaction
                .next_frame
                .accessed_element_states
                .len(),
            tab_handle_index: self.interaction.next_frame.tab_stops.paint_index(),
            line_layout_index: self.text_system.layout_index(),
        }
    }

    pub(crate) fn reuse_paint(&mut self, range: Range<PaintIndex>) {
        #[cfg(feature = "frame-diagnostics")]
        let replay_started_at = Instant::now();
        // Cached elements still exist in the frame even when their paint methods don't run.
        #[cfg(any(test, feature = "test-support"))]
        for (selector, bounds) in &self.interaction.rendered_frame.debug_bounds_records
            [range.start.debug_bounds_index..range.end.debug_bounds_index]
        {
            self.interaction
                .next_frame
                .record_debug_bounds(selector.clone(), *bounds);
        }
        self.interaction.next_frame.cursor_styles.extend(
            self.interaction.rendered_frame.cursor_styles
                [range.start.cursor_styles_index..range.end.cursor_styles_index]
                .iter()
                .cloned(),
        );
        self.text_input
            .reuse_handlers(range.start.input_handlers_index..range.end.input_handlers_index);
        self.interaction.next_frame.mouse_listeners.extend(
            self.interaction.rendered_frame.mouse_listeners
                [range.start.mouse_listeners_index..range.end.mouse_listeners_index]
                .iter_mut()
                .map(|listener| listener.take()),
        );
        self.interaction.next_frame.accessed_element_states.extend(
            self.interaction.rendered_frame.accessed_element_states[range
                .start
                .accessed_element_states_index
                ..range.end.accessed_element_states_index]
                .iter()
                .map(|(id, type_id)| (id.clone(), *type_id)),
        );
        self.interaction.next_frame.tab_stops.replay(
            &self.interaction.rendered_frame.tab_stops.insertion_history
                [range.start.tab_handle_index..range.end.tab_handle_index],
        );

        self.text_system
            .reuse_layouts(range.start.line_layout_index..range.end.line_layout_index);
        self.interaction.next_frame.scene.replay(
            range.start.scene_index..range.end.scene_index,
            &self.interaction.rendered_frame.scene,
        );
        #[cfg(feature = "frame-diagnostics")]
        self.record_frame_phase(
            FramePhase::PaintCacheReplay,
            replay_started_at.elapsed(),
            1,
            1,
        );
    }
}
