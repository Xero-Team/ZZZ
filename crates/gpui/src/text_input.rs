use crate::{Bounds, Pixels, PlatformInputHandler, Point, UTF16Selection};
use std::ops::Range;

/// Narrow text-input capability used by the platform IME bridge.
///
/// The client exposes only candidate geometry; mutation and UTF-16 selection remain behind the
/// existing `InputHandler` compatibility adapter until the platform owners are migrated.
#[allow(
    dead_code,
    reason = "the staged client seam is consumed incrementally by platform adapters"
)]
pub(crate) trait TextInputClient {
    fn selected_text_range(&mut self, ignore_disabled_input: bool) -> Option<UTF16Selection>;
    fn marked_text_range(&mut self) -> Option<Range<usize>>;
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> Option<String>;
    fn replace_text_in_range(&mut self, replacement_range: Option<Range<usize>>, text: &str);
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
    );
    fn unmark_text(&mut self);
    fn bounds_for_range(&mut self, range_utf16: Range<usize>) -> Option<Bounds<Pixels>>;
    fn character_index_for_point(&mut self, point: Point<Pixels>) -> Option<usize>;
    fn accepts_text_input(&mut self) -> bool;
    fn candidate_bounds(&mut self) -> Option<Bounds<Pixels>>;
}

impl TextInputClient for PlatformInputHandler {
    fn selected_text_range(&mut self, ignore_disabled_input: bool) -> Option<UTF16Selection> {
        self.selected_text_range(ignore_disabled_input)
    }

    fn marked_text_range(&mut self) -> Option<Range<usize>> {
        self.marked_text_range()
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> Option<String> {
        self.text_for_range(range_utf16, adjusted_range)
    }

    fn replace_text_in_range(&mut self, replacement_range: Option<Range<usize>>, text: &str) {
        self.replace_text_in_range(replacement_range, text)
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
    ) {
        self.replace_and_mark_text_in_range(range_utf16, new_text, new_selected_range)
    }

    fn unmark_text(&mut self) {
        self.unmark_text()
    }

    fn bounds_for_range(&mut self, range_utf16: Range<usize>) -> Option<Bounds<Pixels>> {
        self.bounds_for_range(range_utf16)
    }

    fn character_index_for_point(&mut self, point: Point<Pixels>) -> Option<usize> {
        self.character_index_for_point(point)
    }

    fn accepts_text_input(&mut self) -> bool {
        self.query_accepts_text_input()
    }

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
        self.active_client()?.candidate_bounds()
    }

    pub(crate) fn active_client(&mut self) -> Option<&mut dyn TextInputClient> {
        self.rendered_handlers.iter_mut().rev().find_map(|handler| {
            handler
                .as_mut()
                .map(|handler| handler as &mut dyn TextInputClient)
        })
    }

    pub(crate) fn reuse_handlers(&mut self, range: Range<usize>) {
        self.next_handlers.extend(
            self.rendered_handlers[range]
                .iter_mut()
                .map(|handler| handler.take()),
        );
    }
}
