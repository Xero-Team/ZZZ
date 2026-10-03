use crate::window::{AnyObserver, AnyWindowFocusListener, HitTest};
use crate::{
    Bounds, BuiltFrame, Capslock, ContentMask, CursorStyle, FocusId, Frame, Hitbox, HitboxBehavior,
    HitboxId, Keystroke, Modifiers, Pixels, Point, SubscriberSet, Task, TextInputOwner, Window,
};
use smallvec::SmallVec;

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
