use super::*;

impl Window {
    /// Sets an input handler, such as [`ElementInputHandler`][element_input_handler], which interfaces with the
    /// platform to receive textual input with proper integration with concerns such
    /// as IME interactions. This handler will be active for the upcoming frame until the following frame is
    /// rendered.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    ///
    /// [element_input_handler]: crate::ElementInputHandler
    pub fn handle_input(
        &mut self,
        focus_handle: &FocusHandle,
        input_handler: impl InputHandler,
        cx: &App,
    ) {
        self.invalidator.debug_assert_paint();

        if focus_handle.is_focused(self) {
            let cx = self.to_async(cx);
            self.text_input
                .next_handlers
                .push(Some(TextInputClient::new(cx, Box::new(input_handler))));
        }
    }

    /// Register a mouse event listener on the window for the next frame. The type of event
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_mouse_event<Event: MouseEvent>(
        &mut self,
        mut listener: impl FnMut(&Event, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.interaction
            .next_frame
            .mouse_listeners
            .push(Some(Box::new(
                move |event: &dyn Any, phase: DispatchPhase, window: &mut Window, cx: &mut App| {
                    if let Some(event) = event.downcast_ref() {
                        listener(event, phase, window, cx)
                    }
                },
            )));
    }

    /// Register a key event listener on this node for the next frame. The type of event
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using event handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_key_event<Event: KeyEvent>(
        &mut self,
        listener: impl Fn(&Event, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.interaction
            .next_frame
            .dispatch_tree
            .on_key_event(Rc::new(
                move |event: &dyn Any, phase, window: &mut Window, cx: &mut App| {
                    if let Some(event) = event.downcast_ref::<Event>() {
                        listener(event, phase, window, cx)
                    }
                },
            ));
    }

    /// Register a modifiers changed event listener on the window for the next frame.
    ///
    /// This is a fairly low-level method, so prefer using event handlers on elements unless you have
    /// a specific need to register a global listener.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_modifiers_changed(
        &mut self,
        listener: impl Fn(&ModifiersChangedEvent, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.interaction
            .next_frame
            .dispatch_tree
            .on_modifiers_changed(Rc::new(
                move |event: &ModifiersChangedEvent, window: &mut Window, cx: &mut App| {
                    listener(event, window, cx)
                },
            ));
    }

    /// Register a listener to be called when the given focus handle or one of its descendants receives focus.
    /// This does not fire if the given focus handle - or one of its descendants - was previously focused.
    /// Returns a subscription and persists until the subscription is dropped.
    pub fn on_focus_in(
        &mut self,
        handle: &FocusHandle,
        cx: &mut App,
        mut listener: impl FnMut(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let focus_id = handle.id;
        let (subscription, activate) =
            self.new_focus_listener(Box::new(move |event, window, cx| {
                if event.is_focus_in(focus_id) {
                    listener(window, cx);
                }
                true
            }));
        cx.defer(move |_| activate());
        subscription
    }

    /// Register a listener to be called when the given focus handle or one of its descendants loses focus.
    /// Returns a subscription and persists until the subscription is dropped.
    pub fn on_focus_out(
        &mut self,
        handle: &FocusHandle,
        cx: &mut App,
        mut listener: impl FnMut(FocusOutEvent, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let focus_id = handle.id;
        let (subscription, activate) =
            self.new_focus_listener(Box::new(move |event, window, cx| {
                if let Some(blurred_id) = event.previous_focus_path.last().copied()
                    && event.is_focus_out(focus_id)
                {
                    let event = FocusOutEvent {
                        blurred: WeakFocusHandle {
                            id: blurred_id,
                            handles: Arc::downgrade(&cx.focus_handles),
                        },
                    };
                    listener(event, window, cx)
                }
                true
            }));
        cx.defer(move |_| activate());
        subscription
    }

    pub(super) fn reset_cursor_style(&self, cx: &mut App) {
        // Set the cursor only if we're the active window.
        if self.is_window_hovered() {
            // A drag can start before the next paint records its cursor in the frame.
            let style = cx
                .active_drag_cursor_style()
                .or_else(|| self.interaction.cursor_style(self))
                .unwrap_or(CursorStyle::Arrow);
            cx.platform.set_cursor_style(style);
        }
    }

    /// Dispatch a given keystroke as though the user had typed it.
    /// You can create a keystroke with Keystroke::parse("").
    pub fn dispatch_keystroke(&mut self, keystroke: Keystroke, cx: &mut App) -> bool {
        let keystroke = keystroke.with_simulated_ime();
        let result = self.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        if !result.propagate {
            return true;
        }

        if let Some(input) = keystroke.key_char {
            #[cfg(feature = "frame-diagnostics")]
            let input_scope = self
                .invalidator
                .input_scope(Instant::now(), FrameInputProvenance::Keyboard);
            let handled = self.dispatch_text_input(&input, cx);
            #[cfg(feature = "frame-diagnostics")]
            drop(input_scope);
            return handled;
        }

        false
    }

    /// Return a key binding string for an action, to display in the UI. Uses the highest precedence
    /// binding for the action (last binding added to the keymap).
    pub fn keystroke_text_for(&self, action: &dyn Action) -> String {
        self.highest_precedence_binding_for_action(action)
            .map_or_else(
                || action.name().to_owned(),
                |binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            )
    }

    /// Dispatch a mouse or keyboard event on the window.
    #[profiling::function]
    pub fn dispatch_event(&mut self, event: PlatformInput, cx: &mut App) -> DispatchEventResult {
        #[cfg(feature = "input-latency-histogram")]
        let dispatch_time = Instant::now();
        #[cfg(feature = "frame-diagnostics")]
        let frame_input_scope = self.invalidator.input_scope(
            Instant::now(),
            match &event {
                PlatformInput::KeyDown(_)
                | PlatformInput::KeyUp(_)
                | PlatformInput::ModifiersChanged(_) => FrameInputProvenance::Keyboard,
                PlatformInput::MouseMove(_)
                | PlatformInput::MouseDown(_)
                | PlatformInput::MouseUp(_)
                | PlatformInput::MousePressure(_)
                | PlatformInput::MouseExited(_)
                | PlatformInput::ScrollWheel(_)
                | PlatformInput::Pinch(_)
                | PlatformInput::FileDrop(_) => FrameInputProvenance::Pointer,
            },
        );
        let update_count_before = self.invalidator.update_count();
        // Track input modality for focus-visible styling and hover suppression.
        // Hover is suppressed during keyboard modality so that keyboard navigation
        // doesn't show hover highlights on the item under the mouse cursor.
        let old_modality = self.interaction.last_input_modality;
        self.interaction.last_input_modality = match &event {
            PlatformInput::KeyDown(_) => InputModality::Keyboard,
            PlatformInput::MouseMove(_) | PlatformInput::MouseDown(_) => InputModality::Mouse,
            _ => self.interaction.last_input_modality,
        };
        if self.interaction.last_input_modality != old_modality {
            self.refresh();
        }

        // Handlers may set this to false by calling `stop_propagation`.
        cx.propagate_event = true;
        // Handlers may set this to true by calling `prevent_default`.
        self.interaction.default_prevented = false;

        let event = match event {
            // Track the mouse position with our own state, since accessing the platform
            // API for the mouse position can only occur on the main thread.
            PlatformInput::MouseMove(mouse_move) => {
                self.interaction.mouse_position = mouse_move.position;
                self.interaction.modifiers = mouse_move.modifiers;
                PlatformInput::MouseMove(mouse_move)
            }
            PlatformInput::MouseDown(mouse_down) => {
                self.interaction.mouse_position = mouse_down.position;
                self.interaction.modifiers = mouse_down.modifiers;
                PlatformInput::MouseDown(mouse_down)
            }
            PlatformInput::MouseUp(mouse_up) => {
                self.interaction.mouse_position = mouse_up.position;
                self.interaction.modifiers = mouse_up.modifiers;
                PlatformInput::MouseUp(mouse_up)
            }
            PlatformInput::MousePressure(mouse_pressure) => {
                PlatformInput::MousePressure(mouse_pressure)
            }
            PlatformInput::MouseExited(mouse_exited) => {
                self.interaction.modifiers = mouse_exited.modifiers;
                PlatformInput::MouseExited(mouse_exited)
            }
            PlatformInput::ModifiersChanged(modifiers_changed) => {
                self.interaction.modifiers = modifiers_changed.modifiers;
                self.interaction.capslock = modifiers_changed.capslock;
                PlatformInput::ModifiersChanged(modifiers_changed)
            }
            PlatformInput::ScrollWheel(scroll_wheel) => {
                self.interaction.mouse_position = scroll_wheel.position;
                self.interaction.modifiers = scroll_wheel.modifiers;
                PlatformInput::ScrollWheel(scroll_wheel)
            }
            PlatformInput::Pinch(pinch) => {
                self.interaction.mouse_position = pinch.position;
                self.interaction.modifiers = pinch.modifiers;
                PlatformInput::Pinch(pinch)
            }
            // Translate dragging and dropping of external files from the operating system
            // to internal drag and drop events.
            PlatformInput::FileDrop(file_drop) => match file_drop {
                FileDropEvent::Entered { position, paths } => {
                    self.interaction.mouse_position = position;
                    if cx.active_drag.is_none() {
                        cx.active_drag = Some(AnyDrag {
                            value: Arc::new(paths.clone()),
                            view: cx.new(|_| paths).into(),
                            cursor_offset: position,
                            cursor_style: None,
                        });
                    }
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    })
                }
                FileDropEvent::Pending { position } => {
                    self.interaction.mouse_position = position;
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    })
                }
                FileDropEvent::Submit { position } => {
                    cx.activate(true).log_err();
                    self.interaction.mouse_position = position;
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    })
                }
                FileDropEvent::Exited => {
                    cx.active_drag.take();
                    PlatformInput::FileDrop(FileDropEvent::Exited)
                }
            },
            PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_) => event,
        };

        if let Some(any_mouse_event) = event.mouse_event() {
            self.dispatch_mouse_event(any_mouse_event, cx);
        } else if let Some(any_key_event) = event.keyboard_event() {
            self.dispatch_key_event(any_key_event, cx);
        }

        if self.invalidator.update_count() > update_count_before {
            self.input_rate_tracker.borrow_mut().record_input();
            #[cfg(feature = "input-latency-histogram")]
            if self.invalidator.not_drawing() {
                self.input_latency_tracker.record_input(dispatch_time);
            } else {
                self.input_latency_tracker.record_mid_draw_input();
            }
        }

        #[cfg(feature = "frame-diagnostics")]
        drop(frame_input_scope);
        DispatchEventResult {
            propagate: cx.propagate_event,
            default_prevented: self.interaction.default_prevented,
        }
    }

    fn dispatch_mouse_event(&mut self, event: &dyn Any, cx: &mut App) {
        self.interaction
            .update_mouse_hit_test(self.mouse_position());

        #[cfg(any(feature = "inspector", debug_assertions))]
        if self.is_inspector_picking(cx) {
            self.reset_cursor_style(cx);
            self.handle_inspector_mouse_event(event, cx);
            // When inspector is picking, all other mouse handling is skipped.
            return;
        }

        InteractionOwner::dispatch_mouse_listeners(self, event, cx);

        // Runs after the listeners so that a drag they start takes effect in this event. Resolving
        // the cursor any earlier would briefly select whatever is under the pointer instead.
        self.reset_cursor_style(cx);
    }

    // If modifiers are not excessive (e.g. AltGr), and the input handler is accepting text input,
    // we prefer the text input over bindings.
    pub(crate) fn input_preference(&mut self, event: &dyn Any, cx: &mut App) -> InputPreference {
        let prefer_character_input = event
            .downcast_ref::<KeyDownEvent>()
            .is_some_and(|key_down_event| key_down_event.prefer_character_input);
        if !prefer_character_input {
            return InputPreference::KeyBindings;
        }
        if self.accepts_text_input(cx) {
            InputPreference::CharacterInput
        } else {
            InputPreference::KeyBindings
        }
    }

    pub(crate) fn accepts_text_input(&mut self, cx: &mut App) -> bool {
        let Some(mut input_handler) = self.platform_window.take_input_handler() else {
            return false;
        };
        let accepts_text_input = input_handler.accepts_text_input(self, cx);
        self.platform_window.set_input_handler(input_handler);
        accepts_text_input
    }

    pub(crate) fn dispatch_text_input(&mut self, input: &str, cx: &mut App) -> bool {
        let Some(mut input_handler) = self.platform_window.take_input_handler() else {
            return false;
        };
        input_handler.dispatch_input(input, self, cx);
        self.platform_window.set_input_handler(input_handler);
        true
    }

    fn dispatch_key_event(&mut self, event: &dyn Any, cx: &mut App) {
        InteractionOwner::dispatch_key_event(self, event, cx);
    }

    pub(crate) fn pending_input_changed(&mut self, cx: &mut App) {
        self.interaction
            .pending_input_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    /// Determine whether a potential multi-stroke key binding is in progress on this window.
    pub fn has_pending_keystrokes(&self) -> bool {
        self.interaction.active_pending_input().is_some()
    }

    pub(crate) fn clear_pending_keystrokes(&mut self, cx: &mut App) {
        InteractionOwner::clear_pending_keystrokes(self, cx);
    }

    /// Returns the currently pending input keystrokes that might result in a multi-stroke key binding.
    pub fn pending_input_keystrokes(&self) -> Option<&[Keystroke]> {
        self.interaction
            .active_pending_input()
            .map(|pending_input| pending_input.keystrokes.as_slice())
    }

    pub(super) fn focus_node_id_in_rendered_frame(
        &self,
        focus_id: Option<FocusId>,
    ) -> DispatchNodeId {
        self.interaction.focus_node_id_in_rendered_frame(focus_id)
    }

    pub(super) fn dispatch_action_on_node(
        &mut self,
        node_id: DispatchNodeId,
        action: &dyn Action,
        cx: &mut App,
    ) {
        InteractionOwner::dispatch_action_on_node(self, node_id, action, cx);
    }

    /// Register the given handler to be invoked whenever the global of the given type
    /// is updated.
    pub fn observe_global<G: Global>(
        &mut self,
        cx: &mut App,
        f: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Subscription {
        let window_handle = self.handle;
        let (subscription, activate) = cx.global_observers.insert(
            TypeId::of::<G>(),
            Box::new(move |cx| {
                window_handle
                    .update(cx, |_, window, cx| f(window, cx))
                    .is_ok()
            }),
        );
        cx.defer(move |_| activate());
        subscription
    }

    /// Focus the current window and bring it to the foreground at the platform level.
    pub fn activate_window(&self) {
        self.platform_window.activate();
    }

    /// Requests that the operating system draw attention to this window.
    pub fn request_attention(&self) {
        if self
            .platform_window
            .capabilities()
            .window_controls
            .attention
        {
            self.platform_window.request_attention();
        }
    }

    /// Minimize the current window at the platform level.
    ///
    /// Returns an error when the platform backend cannot minimize the window.
    pub fn minimize_window(&self) -> Result<()> {
        self.platform_window.minimize()
    }

    /// Toggle full screen status on the current window at the platform level.
    pub fn toggle_fullscreen(&self) {
        if self
            .platform_window
            .capabilities()
            .window_controls
            .fullscreen
        {
            self.platform_window.toggle_fullscreen();
        }
    }

    /// Toggle simple (borderless) fullscreen, where the window covers the entire
    /// screen including the menu bar and, on notched displays, the area around the
    /// notch. Unlike [`Window::toggle_fullscreen`], this does not move the window
    /// into its own Mission Control space. Only has an effect on macOS.
    pub fn toggle_simple_fullscreen(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .simple_fullscreen
        {
            self.platform_window.toggle_simple_fullscreen();
        }
    }

    /// Updates the IME panel position suggestions for languages like japanese, chinese.
    pub fn invalidate_character_coordinates(&self) {
        if !self.platform_window.capabilities().ime_candidate_position {
            return;
        }
        self.on_next_frame(|window, _cx| {
            if let Some(bounds) = window.text_input.candidate_bounds() {
                window.platform_window.update_ime_position(bounds);
            }
        });
    }

    /// Present a platform dialog.
    /// The provided message will be presented, along with buttons for each answer.
    /// When a button is clicked, the returned Receiver will receive the index of the clicked button.
    pub fn prompt<T>(
        &mut self,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        answers: &[T],
        cx: &mut App,
    ) -> oneshot::Receiver<usize>
    where
        T: Clone + Into<PromptButton>,
    {
        let prompt_builder = cx.prompt_builder.take();
        let Some(prompt_builder) = prompt_builder else {
            unreachable!("Re-entrant window prompting is not supported by GPUI");
        };

        let answers = answers
            .iter()
            .map(|answer| answer.clone().into())
            .collect::<Vec<_>>();

        let receiver = match &prompt_builder {
            PromptBuilder::Default if self.platform_window.capabilities().native_prompt => self
                .platform_window
                .prompt(level, message, detail, &answers)
                .unwrap_or_else(|| {
                    self.build_custom_prompt(&prompt_builder, level, message, detail, &answers, cx)
                }),
            PromptBuilder::Custom(_) => {
                self.build_custom_prompt(&prompt_builder, level, message, detail, &answers, cx)
            }
            PromptBuilder::Default => {
                self.build_custom_prompt(&prompt_builder, level, message, detail, &answers, cx)
            }
        };

        cx.prompt_builder = Some(prompt_builder);

        receiver
    }

    fn build_custom_prompt(
        &mut self,
        prompt_builder: &PromptBuilder,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
        cx: &mut App,
    ) -> oneshot::Receiver<usize> {
        let (sender, receiver) = oneshot::channel();
        let handle = PromptHandle::new(sender);
        let handle = (prompt_builder)(level, message, detail, answers, handle, self, cx);
        self.prompt = Some(handle);
        receiver
    }

    /// Returns whether a prompt rendered by GPUI is currently active in this window.
    ///
    /// This is only true for prompts rendered in the window (see
    /// [`App::set_prompt_builder`]), not for platform-native prompt dialogs.
    pub fn has_active_prompt(&self) -> bool {
        self.prompt.is_some()
    }

    /// Returns the current context stack.
    pub fn context_stack(&self) -> Vec<KeyContext> {
        let node_id = self.focus_node_id_in_rendered_frame(self.interaction.focus);
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        dispatch_tree
            .dispatch_path(node_id)
            .iter()
            .filter_map(move |&node_id| dispatch_tree.node(node_id).context.clone())
            .collect()
    }

    /// Returns all available actions for the focused element.
    pub fn available_actions(&self, cx: &App) -> Vec<Box<dyn Action>> {
        let node_id = self.focus_node_id_in_rendered_frame(self.interaction.focus);
        let mut actions = self
            .interaction
            .rendered_frame
            .dispatch_tree
            .available_actions(node_id);
        for action_type in cx.global_action_listeners.keys() {
            if let Err(ix) = actions.binary_search_by_key(action_type, |a| a.as_any().type_id()) {
                let action = cx.actions.build_action_type(action_type).ok();
                if let Some(action) = action {
                    actions.insert(ix, action);
                }
            }
        }
        actions
    }

    /// Returns key bindings that invoke an action on the currently focused element. Bindings are
    /// returned in the order they were added. For display, the last binding should take precedence.
    pub fn bindings_for_action(&self, action: &dyn Action) -> Vec<KeyBinding> {
        self.interaction
            .rendered_frame
            .dispatch_tree
            .bindings_for_action(
                action,
                &self.interaction.rendered_frame.dispatch_tree.context_stack,
            )
    }

    /// Returns the highest precedence key binding that invokes an action on the currently focused
    /// element. This is more efficient than getting the last result of `bindings_for_action`.
    pub fn highest_precedence_binding_for_action(&self, action: &dyn Action) -> Option<KeyBinding> {
        self.interaction
            .rendered_frame
            .dispatch_tree
            .highest_precedence_binding_for_action(
                action,
                &self.interaction.rendered_frame.dispatch_tree.context_stack,
            )
    }

    /// Returns the key bindings for an action in a context.
    pub fn bindings_for_action_in_context(
        &self,
        action: &dyn Action,
        context: KeyContext,
    ) -> Vec<KeyBinding> {
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        dispatch_tree.bindings_for_action(action, &[context])
    }

    /// Returns the highest precedence key binding for an action in a context. This is more
    /// efficient than getting the last result of `bindings_for_action_in_context`.
    pub fn highest_precedence_binding_for_action_in_context(
        &self,
        action: &dyn Action,
        context: KeyContext,
    ) -> Option<KeyBinding> {
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        dispatch_tree.highest_precedence_binding_for_action(action, &[context])
    }

    /// Returns any bindings that would invoke an action on the given focus handle if it were
    /// focused. Bindings are returned in the order they were added. For display, the last binding
    /// should take precedence.
    pub fn bindings_for_action_in(
        &self,
        action: &dyn Action,
        focus_handle: &FocusHandle,
    ) -> Vec<KeyBinding> {
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        let Some(context_stack) = self.context_stack_for_focus_handle(focus_handle) else {
            return vec![];
        };
        dispatch_tree.bindings_for_action(action, &context_stack)
    }

    /// Returns the highest precedence key binding that would invoke an action on the given focus
    /// handle if it were focused. This is more efficient than getting the last result of
    /// `bindings_for_action_in`.
    pub fn highest_precedence_binding_for_action_in(
        &self,
        action: &dyn Action,
        focus_handle: &FocusHandle,
    ) -> Option<KeyBinding> {
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        let context_stack = self.context_stack_for_focus_handle(focus_handle)?;
        dispatch_tree.highest_precedence_binding_for_action(action, &context_stack)
    }

    /// Find the bindings that can follow the current input sequence for the current context stack.
    pub fn possible_bindings_for_input(&self, input: &[Keystroke]) -> Vec<KeyBinding> {
        self.interaction
            .rendered_frame
            .dispatch_tree
            .possible_next_bindings_for_input(input, &self.context_stack())
    }

    fn context_stack_for_focus_handle(
        &self,
        focus_handle: &FocusHandle,
    ) -> Option<Vec<KeyContext>> {
        let dispatch_tree = &self.interaction.rendered_frame.dispatch_tree;
        let node_id = dispatch_tree.focusable_node_id(focus_handle.id)?;
        let context_stack: Vec<_> = dispatch_tree
            .dispatch_path(node_id)
            .into_iter()
            .filter_map(|node_id| dispatch_tree.node(node_id).context.clone())
            .collect();
        Some(context_stack)
    }

    /// Returns a generic event listener that invokes the given listener with the view and context associated with the given view handle.
    pub fn listener_for<T: 'static, E>(
        &self,
        view: &Entity<T>,
        f: impl Fn(&mut T, &E, &mut Window, &mut Context<T>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let view = view.downgrade();
        move |e: &E, window: &mut Window, cx: &mut App| {
            view.update(cx, |view, cx| f(view, e, window, cx)).ok();
        }
    }

    /// Returns a generic handler that invokes the given handler with the view and context associated with the given view handle.
    pub fn handler_for<E: 'static, Callback: Fn(&mut E, &mut Window, &mut Context<E>) + 'static>(
        &self,
        entity: &Entity<E>,
        f: Callback,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let entity = entity.downgrade();
        move |window: &mut Window, cx: &mut App| {
            entity.update(cx, |entity, cx| f(entity, window, cx)).ok();
        }
    }

    /// Register a callback that can interrupt the closing of the current window based the returned boolean.
    /// If the callback returns false, the window won't be closed.
    pub fn on_window_should_close(
        &self,
        cx: &App,
        f: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) {
        let mut cx = self.to_async(cx);
        self.platform_window.on_should_close(Box::new(move || {
            cx.update(|window, cx| f(window, cx)).unwrap_or(true)
        }))
    }

    /// Register an action listener on this node for the next frame. The type of action
    /// is determined by the first parameter of the given listener. When the next frame is rendered
    /// the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using action handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_action(
        &mut self,
        action_type: TypeId,
        listener: impl Fn(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        self.interaction
            .next_frame
            .dispatch_tree
            .on_action(action_type, Rc::new(listener));
    }

    /// Register a capturing action listener on this node for the next frame if the condition is true.
    /// The type of action is determined by the first parameter of the given listener. When the next
    /// frame is rendered the listener will be cleared.
    ///
    /// This is a fairly low-level method, so prefer using action handlers on elements unless you have
    /// a specific need to register a listener yourself.
    ///
    /// This method should only be called as part of the paint phase of element drawing.
    pub fn on_action_when(
        &mut self,
        condition: bool,
        action_type: TypeId,
        listener: impl Fn(&dyn Any, DispatchPhase, &mut Window, &mut App) + 'static,
    ) {
        self.invalidator.debug_assert_paint();

        if condition {
            self.interaction
                .next_frame
                .dispatch_tree
                .on_action(action_type, Rc::new(listener));
        }
    }

    /// Read information about the GPU backing this window.
    /// Currently returns None on Mac and Windows.
    pub fn gpu_specs(&self) -> Option<GpuSpecs> {
        self.platform_window.gpu_specs()
    }

    /// Perform titlebar double-click action.
    /// This is macOS specific.
    pub fn titlebar_double_click(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .titlebar_actions
        {
            self.platform_window.titlebar_double_click();
        }
    }

    /// Gets the window's title at the platform level.
    /// This is macOS specific.
    pub fn window_title(&self) -> String {
        self.platform_window.get_title()
    }

    /// Returns a list of all tabbed windows and their titles.
    /// This is macOS specific.
    pub fn tabbed_windows(&self) -> Option<Vec<SystemWindowTab>> {
        self.platform_window.tabbed_windows()
    }

    /// Returns the tab bar visibility.
    /// This is macOS specific.
    pub fn tab_bar_visible(&self) -> bool {
        self.platform_window.tab_bar_visible()
    }

    /// Merges all open windows into a single tabbed window.
    /// This is macOS specific.
    pub fn merge_all_windows(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .system_tabs
        {
            self.platform_window.merge_all_windows()
        }
    }

    /// Moves the tab to a new containing window.
    /// This is macOS specific.
    pub fn move_tab_to_new_window(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .system_tabs
        {
            self.platform_window.move_tab_to_new_window()
        }
    }

    /// Shows or hides the window tab overview.
    /// This is macOS specific.
    pub fn toggle_window_tab_overview(&self) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .system_tabs
        {
            self.platform_window.toggle_window_tab_overview()
        }
    }

    /// Sets the tabbing identifier for the window.
    /// This is macOS specific.
    pub fn set_tabbing_identifier(&self, tabbing_identifier: Option<String>) {
        if self
            .platform_window
            .capabilities()
            .window_services
            .system_tabs
        {
            self.platform_window
                .set_tabbing_identifier(tabbing_identifier)
        }
    }

    /// Request the OS to play an alert sound. On some platforms this is associated
    /// with the window, for others it's just a simple global function call.
    pub fn play_system_bell(&self) {
        if self.platform_window.capabilities().system_bell {
            self.platform_window.play_system_bell()
        }
    }

    /// Toggles the inspector mode on this window.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn toggle_inspector(&mut self, cx: &mut App) {
        self.inspector = if self.inspector == None {
            Some(cx.new(|_| Inspector::new()))
        } else {
            self.interaction.rendered_frame.next_inspector_instance_ids = FxHashMap::default();
            self.interaction.rendered_frame.inspector_hitboxes = FxHashMap::default();
            self.interaction.next_frame.next_inspector_instance_ids = FxHashMap::default();
            self.interaction.next_frame.inspector_hitboxes = FxHashMap::default();
            None
        };
        self.refresh();
    }

    /// Returns true if the window is in inspector mode.
    pub fn is_inspector_picking(&self, _cx: &App) -> bool {
        #[cfg(any(feature = "inspector", debug_assertions))]
        {
            if let Some(inspector) = &self.inspector {
                return inspector.read(_cx).is_picking();
            }
        }
        false
    }

    /// Executes the provided function with mutable access to an inspector state.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn with_inspector_state<T: 'static, R>(
        &mut self,
        inspector_id: Option<&crate::InspectorElementId>,
        cx: &mut App,
        f: impl FnOnce(&mut Option<T>, &mut Self) -> R,
    ) -> Option<R> {
        let inspector_id = inspector_id?;
        let inspector = self.inspector.as_ref()?;
        if inspector.read(cx).active_element_id() != Some(inspector_id) {
            return None;
        }
        let inspector = inspector.clone();
        Some(inspector.update(cx, |inspector, _cx| {
            inspector.with_active_element_state(self, f)
        }))
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) fn inspector_enabled(&self) -> bool {
        self.inspector.is_some()
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(crate) fn build_inspector_element_id(
        &mut self,
        path: crate::InspectorElementPath,
    ) -> crate::InspectorElementId {
        self.invalidator.debug_assert_paint_or_prepaint();
        let path = Rc::new(path);
        let next_instance_id = self
            .interaction
            .next_frame
            .next_inspector_instance_ids
            .entry(path.clone())
            .or_insert(0);
        let instance_id = *next_instance_id;
        *next_instance_id += 1;
        crate::InspectorElementId { path, instance_id }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(super) fn prepaint_inspector(
        &mut self,
        inspector_width: Pixels,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if let Some(inspector) = self.inspector.take() {
            let mut inspector_element = AnyView::from(inspector.clone()).into_any_element();
            inspector_element.prepaint_as_root(
                point(self.viewport_size.width - inspector_width, px(0.0)),
                size(inspector_width, self.viewport_size.height).into(),
                self,
                cx,
            );
            self.inspector = Some(inspector);
            Some(inspector_element)
        } else {
            None
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(super) fn paint_inspector(
        &mut self,
        mut inspector_element: Option<AnyElement>,
        cx: &mut App,
    ) {
        if let Some(mut inspector_element) = inspector_element {
            inspector_element.paint(self, cx);
        }
    }

    /// Registers a hitbox that can be used for inspector picking mode, allowing users to select and
    /// inspect UI elements by clicking on them.
    #[cfg(any(feature = "inspector", debug_assertions))]
    pub fn insert_inspector_hitbox(
        &mut self,
        hitbox_id: HitboxId,
        inspector_id: Option<&crate::InspectorElementId>,
        cx: &App,
    ) {
        self.invalidator.debug_assert_paint_or_prepaint();
        if !self.is_inspector_picking(cx) {
            return;
        }
        if let Some(inspector_id) = inspector_id {
            self.interaction
                .next_frame
                .inspector_hitboxes
                .insert(hitbox_id, inspector_id.clone());
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    pub(super) fn paint_inspector_hitbox(&mut self, cx: &App) {
        if let Some(inspector) = self.inspector.as_ref() {
            let inspector = inspector.read(cx);
            if let Some((hitbox_id, _)) =
                self.hovered_inspector_hitbox(inspector, &self.interaction.next_frame)
                && let Some(hitbox) = self
                    .interaction
                    .next_frame
                    .hitboxes
                    .iter()
                    .find(|hitbox| hitbox.id == hitbox_id)
            {
                self.paint_quad(crate::fill(hitbox.bounds, crate::rgba(0x61afef4d)));
            }
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn handle_inspector_mouse_event(&mut self, event: &dyn Any, cx: &mut App) {
        let Some(inspector) = self.inspector.clone() else {
            return;
        };
        if event.downcast_ref::<MouseMoveEvent>().is_some() {
            inspector.update(cx, |inspector, _cx| {
                if let Some((_, inspector_id)) =
                    self.hovered_inspector_hitbox(inspector, &self.interaction.rendered_frame)
                {
                    inspector.hover(inspector_id, self);
                }
            });
        } else if event.downcast_ref::<crate::MouseDownEvent>().is_some() {
            inspector.update(cx, |inspector, _cx| {
                if let Some((_, inspector_id)) =
                    self.hovered_inspector_hitbox(inspector, &self.interaction.rendered_frame)
                {
                    inspector.select(inspector_id, self);
                }
            });
        } else if let Some(event) = event.downcast_ref::<crate::ScrollWheelEvent>() {
            // This should be kept in sync with SCROLL_LINES in x11 platform.
            const SCROLL_LINES: f32 = 3.0;
            const SCROLL_PIXELS_PER_LAYER: f32 = 36.0;
            let delta_y = event
                .delta
                .pixel_delta(px(SCROLL_PIXELS_PER_LAYER / SCROLL_LINES))
                .y;
            if let Some(inspector) = self.inspector.clone() {
                inspector.update(cx, |inspector, _cx| {
                    if let Some(depth) = inspector.pick_depth.as_mut() {
                        *depth += f32::from(delta_y) / SCROLL_PIXELS_PER_LAYER;
                        let max_depth = self.interaction.mouse_hit_test.ids.len() as f32 - 0.5;
                        if *depth < 0.0 {
                            *depth = 0.0;
                        } else if *depth > max_depth {
                            *depth = max_depth;
                        }
                        if let Some((_, inspector_id)) = self
                            .hovered_inspector_hitbox(inspector, &self.interaction.rendered_frame)
                        {
                            inspector.set_active_element_id(inspector_id, self);
                        }
                    }
                });
            }
        }
    }

    #[cfg(any(feature = "inspector", debug_assertions))]
    fn hovered_inspector_hitbox(
        &self,
        inspector: &Inspector,
        frame: &Frame,
    ) -> Option<(HitboxId, crate::InspectorElementId)> {
        if let Some(pick_depth) = inspector.pick_depth {
            let depth = (pick_depth as i64).try_into().unwrap_or(0);
            let max_skipped = self.interaction.mouse_hit_test.ids.len().saturating_sub(1);
            let skip_count = (depth as usize).min(max_skipped);
            for hitbox_id in self.interaction.mouse_hit_test.ids.iter().skip(skip_count) {
                if let Some(inspector_id) = frame.inspector_hitboxes.get(hitbox_id) {
                    return Some((*hitbox_id, inspector_id.clone()));
                }
            }
        }
        None
    }

    /// For testing: set the current modifier keys state.
    /// This does not generate any events.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.interaction.modifiers = modifiers;
    }

    /// For testing: simulate a mouse move event to the given position.
    /// This dispatches the event through the normal event handling path,
    /// which will trigger hover states and tooltips.
    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_mouse_move(&mut self, position: Point<Pixels>, cx: &mut App) {
        let event = PlatformInput::MouseMove(MouseMoveEvent {
            position,
            modifiers: self.interaction.modifiers,
            pressed_button: None,
        });
        self.dispatch_event(event, cx);
    }
}
