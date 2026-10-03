use crate::window::{AnyObserver, AnyWindowFocusListener, HitTest};
use crate::{Capslock, FocusId, Frame, HitboxId, Keystroke, Modifiers, SubscriberSet, Task};
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
