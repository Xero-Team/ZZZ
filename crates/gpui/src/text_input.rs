use crate::{Bounds, Pixels, PlatformInputHandler};
use std::ops::Range;

/// Narrow text-input capability used by the platform IME bridge.
///
/// The client exposes only candidate geometry; mutation and UTF-16 selection remain behind the
/// existing `InputHandler` compatibility adapter until the platform owners are migrated.
pub(crate) trait TextInputClient {
    fn candidate_bounds(&mut self) -> Option<Bounds<Pixels>>;
}

impl TextInputClient for PlatformInputHandler {
    fn candidate_bounds(&mut self) -> Option<Bounds<Pixels>> {
        self.ime_candidate_bounds()
    }
}

pub(crate) struct TextInputOwner {
    pub(crate) rendered_handlers: Vec<Option<PlatformInputHandler>>,
    pub(crate) next_handlers: Vec<Option<PlatformInputHandler>>,
}

impl TextInputOwner {
    pub(crate) fn new() -> Self {
        Self {
            rendered_handlers: Vec::new(),
            next_handlers: Vec::new(),
        }
    }

    pub(crate) fn restore_rendered_handler(&mut self, handler: PlatformInputHandler) {
        if let Some(slot) = self
            .rendered_handlers
            .iter_mut()
            .rev()
            .find(|handler| handler.is_none())
        {
            *slot = Some(handler);
        } else {
            self.rendered_handlers.push(Some(handler));
        }
    }

    pub(crate) fn candidate_bounds(&mut self) -> Option<Bounds<Pixels>> {
        self.rendered_handlers
            .iter_mut()
            .rev()
            .find_map(|handler| handler.as_mut()?.candidate_bounds())
    }

    pub(crate) fn reuse_handlers(&mut self, range: Range<usize>) {
        self.next_handlers.extend(
            self.rendered_handlers[range]
                .iter_mut()
                .map(|handler| handler.take()),
        );
    }
}
