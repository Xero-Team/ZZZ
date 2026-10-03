use crate::window::{AnyObserver, AnyWindowFocusListener, DispatchPhase, HitTest};
use crate::{
    Action, App, Bounds, BuiltFrame, Capslock, ContentMask, CursorHideMode, CursorStyle,
    DispatchActionListener, DispatchNodeId, FocusId, Frame, Hitbox, HitboxBehavior, HitboxId,
    InputPreference, KeyContext, KeyDownEvent, Keystroke, Modifiers, ModifiersChangedEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Replay, SubscriberSet, Task, TextInputOwner,
    Window,
};
use gpui_util::ResultExt;
use smallvec::SmallVec;
use std::{any::Any, mem, time::Duration};

/// Runtime interaction state owned by a window.
///
/// The frame contents are built and replayed by the window's draw orchestration, while this
/// owner keeps the mutable hit-testing and pointer-capture state that routes input to a frame.
pub(crate) struct InteractionOwner {
    pub(crate) rendered_frame: Frame,
    pub(crate) next_frame: Frame,
    pub(crate) next_hitbox_id: HitboxId,
    pub(crate) mouse_hit_test: HitTest,
    pub(crate) captured_hitbox: Option<HitboxId>,
    pub(crate) focus_listeners: SubscriberSet<(), AnyWindowFocusListener>,
    pub(crate) focus_lost_listeners: SubscriberSet<(), AnyObserver>,
    pub(crate) default_prevented: bool,
    pub(crate) modifiers: Modifiers,
    pub(crate) capslock: Capslock,
    pub(crate) last_input_modality: InputModality,
    pub(crate) focus: Option<FocusId>,
    pub(crate) focus_generation: u64,
    pub(crate) focus_enabled: bool,
    pub(crate) pending_input: Option<PendingInput>,
    pub(crate) pending_modifier: ModifierState,
    pub(crate) pending_input_observers: SubscriberSet<(), AnyObserver>,
}

impl InteractionOwner {
    pub(crate) fn new(
        rendered_frame: Frame,
        next_frame: Frame,
        next_hitbox_id: HitboxId,
        modifiers: Modifiers,
        capslock: Capslock,
    ) -> Self {
        Self {
            rendered_frame,
            next_frame,
            next_hitbox_id,
            mouse_hit_test: HitTest::default(),
            captured_hitbox: None,
            focus_listeners: SubscriberSet::new(),
            focus_lost_listeners: SubscriberSet::new(),
            default_prevented: true,
            modifiers,
            capslock,
            last_input_modality: InputModality::Mouse,
            focus: None,
            focus_generation: 0,
            focus_enabled: true,
            pending_input: None,
            pending_modifier: ModifierState::default(),
            pending_input_observers: SubscriberSet::new(),
        }
    }

    pub(crate) fn insert_hitbox(
        &mut self,
        bounds: Bounds<Pixels>,
        content_mask: ContentMask<Pixels>,
        behavior: HitboxBehavior,
    ) -> Hitbox {
        let id = self.next_hitbox_id;
        self.next_hitbox_id = id.next();
        let hitbox = Hitbox {
            id,
            bounds,
            content_mask,
            behavior,
        };
        self.next_frame.hitboxes.push(hitbox.clone());
        hitbox
    }

    pub(crate) fn update_mouse_hit_test(&mut self, position: Point<Pixels>) -> bool {
        let hit_test = self.rendered_frame.hit_test(position);
        if hit_test == self.mouse_hit_test {
            false
        } else {
            self.mouse_hit_test = hit_test;
            true
        }
    }

    pub(crate) fn push_cursor_style(&mut self, hitbox_id: Option<HitboxId>, style: CursorStyle) {
        self.next_frame
            .cursor_styles
            .push(crate::window::CursorStyleRequest { hitbox_id, style });
    }

    pub(crate) fn cursor_style(&self, window: &Window) -> Option<CursorStyle> {
        self.rendered_frame.cursor_style(window)
    }

    pub(crate) fn dispatch_mouse_listeners(window: &mut Window, event: &dyn Any, cx: &mut App) {
        let mut mouse_listeners = mem::take(&mut window.interaction.rendered_frame.mouse_listeners);

        for listener in &mut mouse_listeners {
            let listener = listener
                .as_mut()
                .expect("value should have the expected type");
            listener(event, DispatchPhase::Capture, window, cx);
            if !cx.propagate_event {
                break;
            }
        }

        if cx.propagate_event {
            for listener in mouse_listeners.iter_mut().rev() {
                let listener = listener
                    .as_mut()
                    .expect("value should have the expected type");
                listener(event, DispatchPhase::Bubble, window, cx);
                if !cx.propagate_event {
                    break;
                }
            }
        }

        window.interaction.rendered_frame.mouse_listeners = mouse_listeners;

        if cx.has_active_drag() {
            if event.is::<MouseMoveEvent>() {
                window.refresh();
            } else if event.is::<MouseUpEvent>() {
                cx.active_drag = None;
                window.refresh();
            }
        }

        if event.is::<MouseUpEvent>() {
            window.interaction.captured_hitbox = None;
        }
    }

    pub(crate) fn focus_node_id_in_rendered_frame(
        &self,
        focus_id: Option<FocusId>,
    ) -> DispatchNodeId {
        focus_id
            .and_then(|focus_id| {
                self.rendered_frame
                    .dispatch_tree
                    .focusable_node_id(focus_id)
            })
            .unwrap_or_else(|| self.rendered_frame.dispatch_tree.root_node_id())
    }

    pub(crate) fn dispatch_action_on_node(
        window: &mut Window,
        node_id: DispatchNodeId,
        action: &dyn Action,
        cx: &mut App,
    ) {
        Self::dispatch_action_on_node_inner(window, node_id, action, cx);

        if !cx.propagate_event
            && cx.cursor_hide_mode == CursorHideMode::OnTypingAndAction
            && window.last_input_was_keyboard()
        {
            cx.platform.hide_cursor_until_mouse_moves();
        }
    }

    fn dispatch_action_on_node_inner(
        window: &mut Window,
        node_id: DispatchNodeId,
        action: &dyn Action,
        cx: &mut App,
    ) {
        let dispatch_path = window
            .interaction
            .rendered_frame
            .dispatch_tree
            .dispatch_path(node_id);

        cx.propagate_event = true;
        if let Some(mut global_listeners) = cx
            .global_action_listeners
            .remove(&action.as_any().type_id())
        {
            for listener in &global_listeners {
                listener(action.as_any(), DispatchPhase::Capture, cx);
                if !cx.propagate_event {
                    break;
                }
            }

            global_listeners.extend(
                cx.global_action_listeners
                    .remove(&action.as_any().type_id())
                    .unwrap_or_default(),
            );

            cx.global_action_listeners
                .insert(action.as_any().type_id(), global_listeners);
        }

        if !cx.propagate_event {
            return;
        }

        for node_id in &dispatch_path {
            let node = window
                .interaction
                .rendered_frame
                .dispatch_tree
                .node(*node_id);
            for DispatchActionListener {
                action_type,
                listener,
            } in node.action_listeners.clone()
            {
                let any_action = action.as_any();
                if action_type == any_action.type_id() {
                    listener(any_action, DispatchPhase::Capture, window, cx);

                    if !cx.propagate_event {
                        return;
                    }
                }
            }
        }

        for node_id in dispatch_path.iter().rev() {
            let node = window
                .interaction
                .rendered_frame
                .dispatch_tree
                .node(*node_id);
            for DispatchActionListener {
                action_type,
                listener,
            } in node.action_listeners.clone()
            {
                let any_action = action.as_any();
                if action_type == any_action.type_id() {
                    cx.propagate_event = false;
                    listener(any_action, DispatchPhase::Bubble, window, cx);

                    if !cx.propagate_event {
                        return;
                    }
                }
            }
        }

        if let Some(mut global_listeners) = cx
            .global_action_listeners
            .remove(&action.as_any().type_id())
        {
            for listener in global_listeners.iter().rev() {
                cx.propagate_event = false;
                listener(action.as_any(), DispatchPhase::Bubble, cx);
                if !cx.propagate_event {
                    break;
                }
            }

            global_listeners.extend(
                cx.global_action_listeners
                    .remove(&action.as_any().type_id())
                    .unwrap_or_default(),
            );

            cx.global_action_listeners
                .insert(action.as_any().type_id(), global_listeners);
        }
    }

    pub(crate) fn dispatch_key_event(window: &mut Window, event: &dyn Any, cx: &mut App) {
        if window.invalidator.is_dirty() {
            window.draw(cx).clear();
        }

        let node_id = window
            .interaction
            .focus_node_id_in_rendered_frame(window.interaction.focus);
        let dispatch_path = window
            .interaction
            .rendered_frame
            .dispatch_tree
            .dispatch_path(node_id);

        let mut keystroke: Option<Keystroke> = None;

        if let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() {
            if event.modifiers.number_of_modifiers() == 0
                && window
                    .interaction
                    .pending_modifier
                    .modifiers
                    .number_of_modifiers()
                    == 1
                && !window.interaction.pending_modifier.saw_other_input
            {
                let key = match window.interaction.pending_modifier.modifiers {
                    modifiers if modifiers.shift => Some("shift"),
                    modifiers if modifiers.control => Some("control"),
                    modifiers if modifiers.alt => Some("alt"),
                    modifiers if modifiers.platform => Some("platform"),
                    modifiers if modifiers.function => Some("function"),
                    _ => None,
                };
                if let Some(key) = key {
                    keystroke = Some(Keystroke {
                        key: key.to_owned(),
                        key_char: None,
                        modifiers: Modifiers::default(),
                    });
                }
            }

            if window
                .interaction
                .pending_modifier
                .modifiers
                .number_of_modifiers()
                == 0
                && event.modifiers.number_of_modifiers() == 1
            {
                window.interaction.pending_modifier.saw_other_input = false
            } else if event.modifiers.number_of_modifiers() > 1 {
                window.interaction.pending_modifier.saw_other_input = true
            }
            window.interaction.pending_modifier.modifiers = event.modifiers
        } else if let Some(key_down_event) = event.downcast_ref::<KeyDownEvent>() {
            window.interaction.pending_modifier.saw_other_input = true;
            keystroke = Some(key_down_event.keystroke.clone());
            if key_down_event.keystroke.key_char.is_some()
                && matches!(
                    cx.cursor_hide_mode,
                    CursorHideMode::OnTyping | CursorHideMode::OnTypingAndAction
                )
            {
                cx.platform.hide_cursor_until_mouse_moves();
            }
        }

        let Some(keystroke) = keystroke else {
            Self::finish_dispatch_key_event(
                window,
                event,
                None,
                InputPreference::KeyBindings,
                dispatch_path,
                window.context_stack(),
                cx,
            );
            return;
        };

        let input_preference = window.input_preference(event, cx);

        cx.propagate_event = true;
        window.dispatch_keystroke_interceptors(
            &keystroke,
            input_preference,
            window.context_stack(),
            cx,
        );
        if !cx.propagate_event {
            Self::finish_dispatch_key_event(
                window,
                event,
                Some(&keystroke),
                input_preference,
                dispatch_path,
                window.context_stack(),
                cx,
            );
            return;
        }

        let mut currently_pending = window.interaction.pending_input.take().unwrap_or_default();
        if currently_pending.focus.is_some() && currently_pending.focus != window.interaction.focus
        {
            currently_pending = PendingInput::default();
        }

        let match_result = window
            .interaction
            .rendered_frame
            .dispatch_tree
            .dispatch_key(
                currently_pending.keystrokes,
                keystroke.clone(),
                &dispatch_path,
            );

        if !match_result.to_replay.is_empty() {
            Self::replay_pending_input(window, match_result.to_replay, cx);
            cx.propagate_event = true;
        }

        if !match_result.pending.is_empty() {
            currently_pending.timer.take();
            currently_pending.keystrokes = match_result.pending;
            currently_pending.focus = window.interaction.focus;

            let text_input_requires_timeout =
                event
                    .downcast_ref::<KeyDownEvent>()
                    .is_some_and(|key_down| {
                        key_down.keystroke.key_char.is_some() && window.accepts_text_input(cx)
                    });

            currently_pending.needs_timeout |=
                match_result.pending_has_binding || text_input_requires_timeout;

            if currently_pending.needs_timeout {
                currently_pending.timer = Some(window.spawn(cx, async move |cx| {
                    cx.background_executor.timer(Duration::from_secs(1)).await;
                    cx.update(move |window, cx| {
                        let Some(currently_pending) = window
                            .interaction
                            .pending_input
                            .take()
                            .filter(|pending| pending.focus == window.interaction.focus)
                        else {
                            return;
                        };

                        let node_id = window
                            .interaction
                            .focus_node_id_in_rendered_frame(window.interaction.focus);
                        let dispatch_path = window
                            .interaction
                            .rendered_frame
                            .dispatch_tree
                            .dispatch_path(node_id);

                        let to_replay = window
                            .interaction
                            .rendered_frame
                            .dispatch_tree
                            .flush_dispatch(currently_pending.keystrokes, &dispatch_path);

                        window.pending_input_changed(cx);
                        Self::replay_pending_input(window, to_replay, cx)
                    })
                    .log_err();
                }));
            } else {
                currently_pending.timer = None;
            }
            window.interaction.pending_input = Some(currently_pending);
            window.pending_input_changed(cx);
            cx.propagate_event = false;
            return;
        }

        let input_preference = window.input_preference(event, cx);
        if input_preference == InputPreference::KeyBindings {
            for binding in match_result.bindings {
                Self::dispatch_action_on_node(window, node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    window.dispatch_keystroke_observers(
                        &keystroke,
                        input_preference,
                        Some(binding.action.as_ref()),
                        match_result.context_stack,
                        cx,
                    );
                    window.pending_input_changed(cx);
                    return;
                }
            }
        }

        Self::finish_dispatch_key_event(
            window,
            event,
            Some(&keystroke),
            input_preference,
            dispatch_path,
            match_result.context_stack,
            cx,
        );
        window.pending_input_changed(cx);
    }

    fn finish_dispatch_key_event(
        window: &mut Window,
        event: &dyn Any,
        recognized_keystroke: Option<&Keystroke>,
        input_preference: InputPreference,
        dispatch_path: SmallVec<[DispatchNodeId; 32]>,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        Self::dispatch_key_down_up_event(window, event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        Self::dispatch_modifiers_changed_event(window, event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        if let Some(keystroke) = recognized_keystroke {
            window.dispatch_keystroke_observers(
                keystroke,
                input_preference,
                None,
                context_stack,
                cx,
            );
        }
    }

    fn dispatch_key_down_up_event(
        window: &mut Window,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        for node_id in dispatch_path {
            let node = window
                .interaction
                .rendered_frame
                .dispatch_tree
                .node(*node_id);

            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Capture, window, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }

        for node_id in dispatch_path.iter().rev() {
            let node = window
                .interaction
                .rendered_frame
                .dispatch_tree
                .node(*node_id);
            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Bubble, window, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    fn dispatch_modifiers_changed_event(
        window: &mut Window,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() else {
            return;
        };
        for node_id in dispatch_path.iter().rev() {
            let node = window
                .interaction
                .rendered_frame
                .dispatch_tree
                .node(*node_id);
            for listener in node.modifiers_changed_listeners.clone() {
                listener(event, window, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    fn replay_pending_input(window: &mut Window, replays: SmallVec<[Replay; 1]>, cx: &mut App) {
        let node_id = window
            .interaction
            .focus_node_id_in_rendered_frame(window.interaction.focus);
        let dispatch_path = window
            .interaction
            .rendered_frame
            .dispatch_tree
            .dispatch_path(node_id);

        'replay: for replay in replays {
            let event = KeyDownEvent {
                keystroke: replay.keystroke.clone(),
                is_held: false,
                prefer_character_input: true,
            };

            cx.propagate_event = true;
            for binding in replay.bindings {
                Self::dispatch_action_on_node(window, node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    window.dispatch_keystroke_observers(
                        &replay.keystroke,
                        InputPreference::KeyBindings,
                        Some(binding.action.as_ref()),
                        Vec::default(),
                        cx,
                    );
                    continue 'replay;
                }
            }

            Self::dispatch_key_down_up_event(window, &event, &dispatch_path, cx);
            if !cx.propagate_event {
                continue 'replay;
            }
            if let Some(input) = replay.keystroke.key_char.as_deref() {
                window.dispatch_text_input(input, cx);
            }
        }
    }

    pub(crate) fn built_frame<'a>(&'a self, text_input: &'a TextInputOwner) -> BuiltFrame<'a> {
        BuiltFrame::new(
            &self.rendered_frame,
            &self.mouse_hit_test,
            self.captured_hitbox,
            self.focus,
            text_input,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum InputModality {
    Mouse,
    Keyboard,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ModifierState {
    pub(crate) modifiers: Modifiers,
    pub(crate) saw_other_input: bool,
}

#[derive(Default, Debug)]
pub(crate) struct PendingInput {
    pub(crate) keystrokes: SmallVec<[Keystroke; 1]>,
    pub(crate) focus: Option<FocusId>,
    pub(crate) timer: Option<Task<()>>,
    pub(crate) needs_timeout: bool,
}
