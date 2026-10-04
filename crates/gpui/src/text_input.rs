use crate::{Bounds, Pixels, TextInputClient};
use std::ops::Range;

pub(crate) struct TextInputOwner {
    pub(crate) rendered_handlers: Vec<Option<TextInputClient>>,
    pub(crate) next_handlers: Vec<Option<TextInputClient>>,
}

impl TextInputOwner {
    pub(crate) fn new() -> Self {
        Self {
            rendered_handlers: Vec::new(),
            next_handlers: Vec::new(),
        }
    }

    pub(crate) fn restore_rendered_handler(&mut self, handler: TextInputClient) {
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
        self.active_client()?.ime_candidate_bounds()
    }

    pub(crate) fn active_client(&mut self) -> Option<&mut TextInputClient> {
        self.rendered_handlers
            .iter_mut()
            .rev()
            .find_map(Option::as_mut)
    }

    pub(crate) fn reuse_handlers(&mut self, range: Range<usize>) {
        self.next_handlers.extend(
            self.rendered_handlers[range]
                .iter_mut()
                .map(|handler| handler.take()),
        );
    }
}
