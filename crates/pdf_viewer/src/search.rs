use std::collections::HashMap;

use gpui::{Context, Window};

use crate::view::{PdfView, PdfViewEvent};

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
    /// worker thread and cached, so repeat searches over the same document only
    /// pay the matching cost.
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
        let mut cached: HashMap<usize, String> = state.search.page_text.clone();

        let task = cx.spawn_in(window, async move |this, cx| {
            let needle = query.to_lowercase();

            for index in 0..page_count {
                let text = if let Some(text) = cached.get(&index) {
                    text.clone()
                } else {
                    match worker
                        .page_text(index, crate::worker::Priority::Foreground)
                        .await
                    {
                        Ok(Ok(text)) => {
                            cached.insert(index, text.clone());
                            text
                        }
                        Ok(Err(error)) => {
                            log::warn!("extracting text for page {index}: {error:#}");
                            cached.insert(index, String::new());
                            String::new()
                        }
                        Err(_canceled) => return,
                    }
                };

                let matched = text.to_lowercase().contains(&needle);
                let stop = this
                    .update_in(cx, |view, window, cx| {
                        let Some(state) = view.loaded_mut() else {
                            return true;
                        };
                        // A newer search superseded this one.
                        if state.search.query.as_ref() != query {
                            return true;
                        }
                        state.search.page_text.insert(index, text.clone());
                        if matched {
                            let is_first = state.search.matches.is_empty();
                            state.search.matches.push(index);
                            if is_first {
                                state.search.active_match = Some(0);
                                view.go_to_page(index, window, cx);
                            }
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
