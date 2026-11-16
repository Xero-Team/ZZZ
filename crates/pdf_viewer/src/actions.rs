use std::time::Duration;

use gpui::{Bounds, Context, Focusable as _, IsZero, Pixels, Window, point, px};
use i18n::tr;

use crate::{
    BASE_DPI, CACHE_MEMORY_BUDGET, CopyPageText, FitPage, FitWidth, MAX_ZOOM, MIN_ZOOM, NextMatch,
    NextPage, PAGE_HORIZONTAL_MARGIN, PAGE_VERTICAL_MARGIN, PreviousMatch, PreviousPage, ResetZoom,
    RotatePageLeft, RotatePageRight, ToggleAnnotate, ToggleSearch, ToggleSidebar, ZOOM_STEP,
    ZoomIn, ZoomOut,
    edit::PdfEdit,
    view::{
        PdfView, PdfViewEvent, ZoomMode, estimate_page_bytes, idle_prefetch_order,
        pages_to_keep_within_budget, prefetch_admits, render_dpi,
    },
    worker::Priority,
};

/// How long the viewport must be still before idle prefetch starts filling the
/// cache outward. Short enough to feel responsive, long enough that it does not
/// compete with active scrolling.
const IDLE_PREFETCH_DELAY: Duration = Duration::from_millis(400);

/// Per-page decision made while expanding idle prefetch outward.
#[derive(Clone, Copy)]
enum PrefetchStep {
    /// Render this page at the given DPI.
    Render(f32),
    /// Already cached or in flight; move on to the next page.
    Skip,
    /// Budget reached or document gone; stop expanding.
    Stop,
}

impl PdfView {
    /// Recompute `zoom` from the active fit mode against the current container
    /// size and current page dimensions, then schedule any needed renders.
    pub(crate) fn recompute_fit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.container_bounds else {
            return;
        };
        let page = self.current_page;
        let Some(state) = self.loaded() else { return };
        let Some(summary_page) = state.summary.pages.get(page) else {
            return;
        };
        let (page_width, page_height) = (summary_page.width as f32, summary_page.height as f32);
        if page_width <= 0.0 || page_height <= 0.0 {
            return;
        }

        // Container points map 1:1 to PDF points at zoom 1.0 (BASE_DPI / 72 is
        // folded into the render DPI, not the layout size).
        let available_width: f32 =
            (f32::from(bounds.size.width) - 2.0 * PAGE_HORIZONTAL_MARGIN).max(1.0);
        let available_height: f32 =
            (f32::from(bounds.size.height) - 2.0 * PAGE_VERTICAL_MARGIN).max(1.0);

        let new_zoom = match self.zoom_mode {
            ZoomMode::Custom => return,
            ZoomMode::FitWidth => (available_width / page_width).clamp(MIN_ZOOM, MAX_ZOOM),
            ZoomMode::FitPage => (available_width / page_width)
                .min(available_height / page_height)
                .clamp(MIN_ZOOM, MAX_ZOOM),
        };

        if (new_zoom - self.zoom).abs() > f32::EPSILON {
            self.zoom = new_zoom;
            self.supersede_renders();
            self.update_page_layouts();
            self.ensure_pages_rendered(window, cx);
            cx.on_next_frame(window, |view, _window, cx| {
                view.scroll_to_current_page(cx);
            });
            cx.notify();
        }
    }

    pub(crate) fn set_container_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let first_layout = self.container_bounds.is_none();
        let changed = self.container_bounds != Some(bounds);
        self.container_bounds = Some(bounds);
        if changed || first_layout {
            self.recompute_fit(window, cx);
            self.update_page_layouts();
            self.ensure_pages_rendered(window, cx);
            cx.on_next_frame(window, |view, _window, cx| {
                view.flush_pending_scroll(cx);
            });
        }
    }

    /// Drop in-flight renders so the next pass re-renders at the current scale, but
    /// keep the bitmaps already on screen: a page whose cached scale is stale still
    /// paints (GPUI stretches it), so zooming never blanks the document.
    fn supersede_renders(&mut self) {
        if let Some(state) = self.loaded_mut() {
            state.rendering.clear();
            state.idle_prefetch = None;
        }
    }

    /// Render the visible pages plus a small immediate prefetch window at
    /// foreground priority, then evict cached pages once the cache exceeds its
    /// memory budget. Also (re)arms idle prefetch so the rest of the document
    /// fills in while the viewport is still.
    pub(crate) fn ensure_pages_rendered(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        const PREFETCH_RADIUS: usize = 1;

        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        let current = self.current_page;
        let (visible_start, visible_end) = self.visible_page_range().unwrap_or((current, current));
        let start = visible_start.saturating_sub(PREFETCH_RADIUS);
        let end = (visible_end + PREFETCH_RADIUS).min(page_count - 1);

        for index in start..=end {
            self.render_page(
                index,
                self.page_dpi(index, window.scale_factor()),
                Priority::Foreground,
                window,
                cx,
            );
        }

        let range_center = usize::midpoint(visible_start, visible_end);
        self.evict_over_budget(range_center);
        self.arm_idle_prefetch(window, cx);
    }

    /// Drop cached pages farthest from `center` until the cached bytes fit
    /// within [`CACHE_MEMORY_BUDGET`]. In-flight renders are left alone.
    fn evict_over_budget(&mut self, center: usize) {
        let Some(state) = self.loaded_mut() else {
            return;
        };
        let cached_bytes: u64 = state
            .page_cache
            .values()
            .map(|preview| preview.bytes())
            .sum();
        if cached_bytes <= CACHE_MEMORY_BUDGET {
            return;
        }
        let pages: Vec<(usize, u64)> = state
            .page_cache
            .iter()
            .map(|(&index, preview)| (index, preview.bytes()))
            .collect();
        let keep: std::collections::HashSet<usize> =
            pages_to_keep_within_budget(pages, center, CACHE_MEMORY_BUDGET)
                .into_iter()
                .collect();
        state.page_cache.retain(|index, _| keep.contains(index));
    }

    fn page_dpi(&self, index: usize, device_scale: f32) -> f32 {
        let fallback = BASE_DPI * self.zoom * device_scale.max(1.0);
        let Some(layout) = self.page_layouts.get(index) else {
            return fallback;
        };
        let Some(state) = self.loaded() else {
            return fallback;
        };
        let Some(summary_page) = state.summary.pages.get(index) else {
            return fallback;
        };
        render_dpi(layout.width, summary_page.width as f32, device_scale)
    }

    /// Render a single page at the given DPI unless an up-to-date bitmap is
    /// already cached or a render is in flight.
    fn render_page(
        &mut self,
        index: usize,
        dpi: f32,
        priority: Priority,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scale = dpi / 72.0;
        let Some(state) = self.loaded() else { return };

        if state
            .page_cache
            .get(&index)
            .is_some_and(|preview| (preview.scale - scale).abs() < f32::EPSILON)
            || state.rendering.contains_key(&index)
        {
            return;
        }

        let worker = state.worker.clone();
        let receiver = worker.render_page(index, dpi, priority);
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |view, cx| {
                let center = view.current_page;
                if let Some(state) = view.loaded_mut() {
                    state.rendering.remove(&index);
                    match result {
                        Ok(Ok(preview)) => {
                            if preview.blank_render_suspected {
                                state.encryption_warning = true;
                            }
                            state.page_cache.insert(index, preview);
                            cx.notify();
                        }
                        Ok(Err(error)) => log::warn!("rendering PDF page {index}: {error:#}"),
                        Err(_canceled) => {}
                    }
                }
                // Trim immediately on completion: idle prefetch inserts here
                // without going through `ensure_pages_rendered`, so this is the
                // authoritative point that keeps the cache within budget.
                view.evict_over_budget(center);
            });
        });

        if let Some(state) = self.loaded_mut() {
            state.rendering.insert(index, task);
        }
    }

    /// Arm (or re-arm) the idle prefetch task: after a short still period, fill
    /// the cache outward from the current page at background priority until the
    /// memory budget is reached. Replacing the stored task cancels any previous
    /// one, so this is cheap to call on every scroll/zoom settle.
    fn arm_idle_prefetch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let page_count = self.page_count();
        if page_count <= 1 {
            return;
        }
        let device_scale = window.scale_factor();
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(IDLE_PREFETCH_DELAY).await;
            // The anchor page can move between expansion steps; capture it once
            // so the budget is measured from where the user actually is, and
            // bail if the document went away.
            let Ok(center) = this.read_with(cx, |view, _| view.current_page) else {
                return;
            };

            for index in idle_prefetch_order(center, page_count) {
                let outcome = this
                    .update_in(cx, |view, window, cx| {
                        // A navigation changed the anchor; the newer arm owns
                        // prefetch from here.
                        if view.current_page != center {
                            return PrefetchStep::Stop;
                        }
                        match view.prefetch_idle_page(index, device_scale) {
                            PrefetchStep::Render(dpi) => {
                                view.render_page(index, dpi, Priority::Background, window, cx);
                                PrefetchStep::Render(dpi)
                            }
                            other => other,
                        }
                    })
                    .unwrap_or(PrefetchStep::Stop);
                match outcome {
                    PrefetchStep::Stop => return,
                    // A cached/in-flight hole shouldn't halt expansion further
                    // out, so keep going without waiting.
                    PrefetchStep::Skip => continue,
                    PrefetchStep::Render(_) => {}
                }
                // Yield between submissions so a burst of navigation can preempt
                // the background fill before the queue fills with stale work.
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
            }
        });
        if let Some(state) = self.loaded_mut() {
            state.idle_prefetch = Some(task);
        }
    }

    /// Decide what idle prefetch should do for page `index`: render it at the
    /// returned DPI, skip it (already cached or in flight) while continuing to
    /// expand, or stop expanding entirely once the budget would be exceeded.
    ///
    /// The projection counts both cached bytes and the estimated bytes of
    /// renders already in flight. Without the in-flight term, idle prefetch
    /// submits faster than renders complete and every submission sees a nearly
    /// empty cache, so it would queue the whole document before the first
    /// bitmaps land — blowing far past the budget.
    fn prefetch_idle_page(&self, index: usize, device_scale: f32) -> PrefetchStep {
        let scale = self.page_dpi(index, device_scale) / 72.0;
        let Some(state) = self.loaded() else {
            return PrefetchStep::Stop;
        };
        let already_current = state
            .page_cache
            .get(&index)
            .is_some_and(|preview| (preview.scale - scale).abs() < f32::EPSILON);
        if already_current || state.rendering.contains_key(&index) {
            return PrefetchStep::Skip;
        }
        let Some(layout) = self.page_layouts.get(index) else {
            return PrefetchStep::Skip;
        };
        let page_bytes = estimate_page_bytes(layout.width, layout.height, scale);
        if !prefetch_admits(
            self.committed_cache_bytes(device_scale),
            page_bytes,
            CACHE_MEMORY_BUDGET,
        ) {
            return PrefetchStep::Stop;
        }
        PrefetchStep::Render(self.page_dpi(index, device_scale))
    }

    /// Bytes already committed to the cache: resident bitmaps plus the estimated
    /// size of every in-flight render (which will land in the cache soon).
    fn committed_cache_bytes(&self, device_scale: f32) -> u64 {
        let Some(state) = self.loaded() else {
            return 0;
        };
        let cached: u64 = state
            .page_cache
            .values()
            .map(|preview| preview.bytes())
            .sum();
        let in_flight: u64 = state
            .rendering
            .keys()
            .filter(|index| !state.page_cache.contains_key(index))
            .filter_map(|&index| {
                let layout = self.page_layouts.get(index)?;
                let scale = self.page_dpi(index, device_scale) / 72.0;
                Some(estimate_page_bytes(layout.width, layout.height, scale))
            })
            .sum();
        cached.saturating_add(in_flight)
    }

    pub(crate) fn go_to_page(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let page_count = self.page_count();
        if page_count == 0 {
            return;
        }
        let index = index.min(page_count - 1);
        if index == self.current_page && self.pending_scroll_to_page.is_none() {
            self.scroll_to_current_page(cx);
            return;
        }
        self.current_page = index;
        self.pending_scroll_to_page = Some(index);
        self.recompute_fit(window, cx);
        self.update_page_layouts();
        self.ensure_pages_rendered(window, cx);
        cx.on_next_frame(window, |view, _window, cx| {
            view.flush_pending_scroll(cx);
        });
        cx.emit(PdfViewEvent::TitleChanged);
        cx.notify();
    }

    pub(crate) fn previous_page(
        &mut self,
        _: &PreviousPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_page > 0 {
            self.go_to_page(self.current_page - 1, window, cx);
        }
    }

    pub(crate) fn next_page(&mut self, _: &NextPage, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to_page(self.current_page + 1, window, cx);
    }

    fn set_zoom(&mut self, zoom: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_mode = ZoomMode::Custom;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let relative_offset = self.relative_scroll_offset();
        self.supersede_renders();
        self.update_page_layouts();
        self.ensure_pages_rendered(window, cx);
        self.restore_relative_scroll_offset(relative_offset);
        cx.on_next_frame(window, |view, _window, cx| {
            view.sync_current_page_with_scroll(cx);
        });
        cx.notify();
    }

    pub(crate) fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom * ZOOM_STEP, window, cx);
    }

    pub(crate) fn zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom / ZOOM_STEP, window, cx);
    }

    pub(crate) fn reset_zoom(
        &mut self,
        _: &ResetZoom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_zoom(1.0, window, cx);
    }

    pub(crate) fn fit_width(&mut self, _: &FitWidth, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_mode = ZoomMode::FitWidth;
        self.recompute_fit(window, cx);
        cx.notify();
    }

    pub(crate) fn fit_page(&mut self, _: &FitPage, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_mode = ZoomMode::FitPage;
        self.recompute_fit(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_sidebar.toggle();
        cx.notify();
    }

    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_open.toggle();
        if self.search_open {
            let handle = self.search_editor.focus_handle(cx);
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn next_match(
        &mut self,
        _: &NextMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_match(1, window, cx);
    }

    pub(crate) fn previous_match(
        &mut self,
        _: &PreviousMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_match(-1, window, cx);
    }

    fn step_match(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.loaded() else { return };
        let count = state.search.matches.len();
        if count == 0 {
            return;
        }
        let current = state.search.active_match.unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(count as isize) as usize;
        let Some(target_page) = state.search.matches.get(next).map(|found| found.page) else {
            return;
        };
        if let Some(state) = self.loaded_mut() {
            state.search.active_match = Some(next);
        }
        self.go_to_page(target_page, window, cx);
    }

    /// Fraction (0..=1) of the current page scrolled past the top of the
    /// viewport, used to preserve the reading position across a zoom change.
    ///
    /// The scroll handle's offset follows the convention that `-offset.y` is the
    /// content-space coordinate currently aligned with the viewport's top edge
    /// (see `ScrollHandle::scroll_to_top_of_item`). Working purely in that
    /// content space keeps this consistent with `update_page_layouts`, which
    /// also measures `top` from the content origin.
    fn relative_scroll_offset(&self) -> f32 {
        let Some(layout) = self.current_page_layout() else {
            return 0.0;
        };
        if layout.height <= 0.0 {
            return 0.0;
        }
        let content_top = -f32::from(self.scroll_handle.offset().y);
        ((content_top - layout.top) / layout.height).clamp(0.0, 1.0)
    }

    fn restore_relative_scroll_offset(&mut self, ratio: f32) {
        let Some(layout) = self.current_page_layout() else {
            return;
        };
        if self.scroll_handle.bounds().size.height.is_zero() {
            self.pending_scroll_to_page = Some(self.current_page);
            return;
        }
        let target_top = layout.top + layout.height * ratio.clamp(0.0, 1.0);
        self.scroll_handle
            .set_offset(point(px(0.0), px(-target_top)));
    }

    /// Write `edits` back to the file, then adopt the result.
    ///
    /// One write runs at a time: two incremental updates built from the same base
    /// bytes would lose one of them. The bytes go to disk before the edit is
    /// published to the worker pool, so a failed write leaves every thread on the
    /// old document.
    pub(crate) fn commit_edits(
        &mut self,
        edits: Vec<PdfEdit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if edits.is_empty() {
            return;
        }
        let Some(state) = self.loaded() else {
            return;
        };
        if state.pending_edit.is_some() {
            return;
        }
        let Some(path) = self.abs_path(cx) else {
            self.fail_edit(
                tr(
                    cx,
                    "pdf_viewer.edit.error_path",
                    "Could not resolve the PDF's path.",
                )
                .into(),
                cx,
            );
            return;
        };
        let worker = state.worker.clone();
        let fs = self.project.read(cx).fs().clone();
        let canceled_message: gpui::SharedString = tr(
            cx,
            "pdf_viewer.edit.error_canceled",
            "The edit was canceled.",
        )
        .into();

        let task = cx.spawn_in(window, async move |this, cx| {
            let outcome = match worker.apply_edits(edits).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(error)) => {
                    Self::report_edit_error(&this, format!("{error:#}"), cx);
                    return;
                }
                Err(_canceled) => {
                    Self::report_edit_error(&this, canceled_message, cx);
                    return;
                }
            };
            if let Err(error) = fs.write(&path, &outcome.bytes).await {
                Self::report_edit_error(&this, format!("{error:#}"), cx);
                return;
            }
            worker.publish(&outcome);
            this.update_in(cx, |view, window, cx| view.on_document_edited(window, cx))
                .ok();
        });

        if let Some(state) = self.loaded_mut() {
            state.pending_edit = Some(task);
        }
        cx.notify();
    }

    /// Record an edit failure on the view. Also used from the sync path, where
    /// the error is known before any task is spawned.
    fn fail_edit(&mut self, message: gpui::SharedString, cx: &mut Context<Self>) {
        if let Some(state) = self.loaded_mut() {
            state.pending_edit = None;
            state.edit_error = Some(message);
        }
        cx.notify();
    }

    fn report_edit_error(
        this: &gpui::WeakEntity<Self>,
        message: impl Into<gpui::SharedString>,
        cx: &mut gpui::AsyncWindowContext,
    ) {
        let message = message.into();
        this.update(cx, |view, cx| {
            if let Some(state) = view.loaded_mut() {
                state.pending_edit = None;
                state.edit_error = Some(message);
            }
            cx.notify();
        })
        .ok();
    }

    /// Adopt a document after an edit landed on disk: new summary, empty caches
    /// (annotations are baked into the bitmaps), and re-laid-out pages.
    pub(crate) fn on_document_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let summary = self.loaded().map(|state| state.worker.summary());
        if let Some(summary) = summary
            && let Some(state) = self.loaded_mut()
        {
            state.summary = summary;
            state.page_cache.clear();
            state.rendering.clear();
            state.annotations.clear();
            state.loading_annotations.clear();
            state.pending_edit = None;
            state.edit_error = None;
            state.selected_annotation = None;
            state.search.matches.clear();
        }
        self.annotate.drag = None;
        self.form.active = None;
        self.current_page = self.current_page.min(self.page_count().saturating_sub(1));
        self.recompute_fit(window, cx);
        self.update_page_layouts();
        self.ensure_pages_rendered(window, cx);
        cx.notify();
    }

    /// Fetch the annotations of the pages on screen, so the overlays can draw
    /// link targets, selectable annotations, and form widgets.
    pub(crate) fn ensure_page_annotations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.loaded() else {
            return;
        };
        let page_count = state.summary.page_count;
        if page_count == 0 {
            return;
        }
        let (first, last) = self
            .visible_page_range()
            .unwrap_or((self.current_page, self.current_page));
        let worker = state.worker.clone();
        let wanted: Vec<usize> = (first..=last.min(page_count - 1))
            .filter(|index| {
                !state.annotations.contains_key(index)
                    && !state.loading_annotations.contains_key(index)
            })
            .collect();

        for index in wanted {
            let receiver = worker.page_annotations(index, Priority::Background);
            let task = cx.spawn_in(window, async move |this, cx| {
                let result = receiver.await;
                this.update(cx, |view, cx| {
                    if let Some(state) = view.loaded_mut() {
                        state.loading_annotations.remove(&index);
                        if let Ok(Ok(annotations)) = result {
                            state.annotations.insert(index, annotations);
                            cx.notify();
                        }
                    }
                })
                .ok();
            });
            if let Some(state) = self.loaded_mut() {
                state.loading_annotations.insert(index, task);
            }
        }
    }

    /// Fetch the metadata, signatures, and attachment list the properties
    /// section shows. Requested once per document revision.
    pub(crate) fn ensure_document_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.loaded() else {
            return;
        };
        if state.details.is_some() || state.details_task.is_some() {
            return;
        }
        let worker = state.worker.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let details = match worker.document_details().await {
                Ok(Ok(details)) => Some(details),
                Ok(Err(error)) => {
                    log::warn!("reading PDF details: {error:#}");
                    None
                }
                Err(_canceled) => None,
            };
            this.update(cx, |view, cx| {
                if let Some(state) = view.loaded_mut() {
                    state.details_task = None;
                    state.details = details;
                    cx.notify();
                }
            })
            .ok();
        });
        if let Some(state) = self.loaded_mut() {
            state.details_task = Some(task);
        }
    }

    /// Rotate the current page by `degrees` (a multiple of 90) and write it back.
    pub(crate) fn rotate_current_page(
        &mut self,
        degrees: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.loaded() else {
            return;
        };
        let current = state
            .summary
            .pages
            .get(self.current_page)
            .map_or(0, |page| page.rotate);
        let target = (current + degrees).rem_euclid(360);
        self.commit_edits(
            vec![PdfEdit::RotatePage {
                page: self.current_page,
                degrees: target,
            }],
            window,
            cx,
        );
    }

    /// Copy the current page's text to the clipboard.
    pub(crate) fn copy_current_page_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.loaded() else {
            return;
        };
        let worker = state.worker.clone();
        let page = self.current_page;
        cx.spawn_in(window, async move |_this, cx| {
            if let Ok(Ok(text)) = worker.page_text(page, Priority::Foreground).await {
                cx.update(|_window, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn toggle_annotate(
        &mut self,
        _: &ToggleAnnotate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.annotate.open = !self.annotate.open;
        self.annotate.drag = None;
        if !self.annotate.open {
            self.annotate.tool = crate::view::AnnotationTool::Select;
        }
        self.ensure_page_annotations(window, cx);
        cx.notify();
    }

    pub(crate) fn copy_page_text(
        &mut self,
        _: &CopyPageText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_current_page_text(window, cx);
    }

    pub(crate) fn rotate_page_left(
        &mut self,
        _: &RotatePageLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rotate_current_page(-90, window, cx);
    }

    pub(crate) fn rotate_page_right(
        &mut self,
        _: &RotatePageRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rotate_current_page(90, window, cx);
    }
}
