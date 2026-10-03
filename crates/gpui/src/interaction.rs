use crate::window::HitTest;
use crate::{Frame, HitboxId};

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
}

impl InteractionOwner {
    pub(crate) fn new(rendered_frame: Frame, next_frame: Frame, next_hitbox_id: HitboxId) -> Self {
        Self {
            rendered_frame,
            next_frame,
            next_hitbox_id,
            mouse_hit_test: HitTest::default(),
            captured_hitbox: None,
        }
    }
}
