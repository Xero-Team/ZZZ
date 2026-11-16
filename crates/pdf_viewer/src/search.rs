use std::collections::HashMap;

use gpui::{Context, Window};
use zpdf::{TextSpan, search_spans};

use crate::view::{PdfView, PdfViewEvent, SearchMatch, normalize_page_rect};

impl PdfView {
    pub(crate) fn on_search_editor_event(
        &mut self,
        _editor: &gpui::Entity<editor::Editor>,
        event: &editor::EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, editor::EditorEvent::BufferEdited) {
            let query = self.search_editor.read(cx).text(cx);
            self.run_search(query, window, cx);
        }
    }

    /// Kick off a full-document text search. Page text is extracted on the
    /// worker thread and kept as spans, so the view can both scroll to a match
    /// and draw it over the page, and a repeat search over the same document
    /// only pays the matching cost.
    pub(crate) fn run_search(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = query.trim().to_owned();
        let Some(state) = self.loaded_mut() else {
            return;
        };
        state.search.query = query.clone().into();
        state.search.matches.clear();
        state.search.active_match = None;

        if query.is_empty() {
            state.search.task = None;
            cx.notify();
            return;
        }

        let worker = state.worker.clone();
        let page_count = state.summary.page_count;
        let page_sizes: Vec<(f32, f32)> = state
            .summary
            .pages
            .iter()
            .map(|page| (page.width as f32, page.height as f32))
            .collect();
        let mut cached: HashMap<usize, Vec<TextSpan>> = state.search.page_spans.clone();

        let task = cx.spawn_in(window, async move |this, cx| {
            for index in 0..page_count {
                let spans = match cached.get(&index).cloned() {
                    Some(spans) => spans,
                    None => match worker
                        .page_spans(index, crate::worker::Priority::Foreground)
                        .await
                    {
                        Ok(Ok(spans)) => {
                            cached.insert(index, spans.clone());
                            spans
                        }
                        Ok(Err(error)) => {
                            log::warn!("extracting text for page {index}: {error:#}");
                            Vec::new()
                        }
                        Err(_canceled) => return,
                    },
                };

                let Some(&(page_width, page_height)) = page_sizes.get(index) else {
                    continue;
                };
                let page_matches: Vec<SearchMatch> = search_spans(&spans, &query, false)
                    .into_iter()
                    .map(|hit| SearchMatch {
                        page: index,
                        rects: hit
                            .rects
                            .iter()
                            .map(|rect| {
                                normalize_page_rect(
                                    (rect.x0, rect.y0, rect.x1, rect.y1),
                                    page_width,
                                    page_height,
                                )
                            })
                            .collect(),
                    })
                    .filter(|found| !found.rects.is_empty())
                    .collect();

                let stop = this
                    .update_in(cx, |view, window, cx| {
                        let Some(state) = view.loaded_mut() else {
                            return true;
                        };
                        // A newer search superseded this one.
                        if state.search.query.as_ref() != query {
                            return true;
                        }
                        state.search.page_spans.insert(index, spans);
                        if page_matches.is_empty() {
                            cx.notify();
                            return false;
                        }
                        let first_match = state.search.matches.is_empty();
                        let first_page = page_matches[0].page;
                        state.search.matches.extend(page_matches);
                        if first_match {
                            state.search.active_match = Some(0);
                            view.go_to_page(first_page, window, cx);
                        }
                        cx.notify();
                        false
                    })
                    .unwrap_or(true);
                if stop {
                    return;
                }
            }
        });

        if let Some(state) = self.loaded_mut() {
            state.search.task = Some(task);
        }
        cx.emit(PdfViewEvent::TitleChanged);
        cx.notify();
    }
}
