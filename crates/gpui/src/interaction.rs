use crate::window::{AnyObserver, AnyWindowFocusListener, DispatchPhase, HitTest};
use crate::{
    Action, App, Bounds, BuiltFrame, Capslock, ContentMask, CursorHideMode, CursorStyle,
    DispatchActionListener, DispatchNodeId, FocusId, Frame, Hitbox, HitboxBehavior, HitboxId,
    Keystroke, Modifiers, MouseMoveEvent, MouseUpEvent, Pixels, Point, SubscriberSet, Task,
    TextInputOwner, Window,
};
use smallvec::SmallVec;
use std::{any::Any, mem};

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
