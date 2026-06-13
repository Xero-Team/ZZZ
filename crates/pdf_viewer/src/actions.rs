use std::time::Duration;

use gpui::{Bounds, Context, Focusable as _, IsZero, Pixels, Window, point, px};

use crate::{
    BASE_DPI, CACHE_MEMORY_BUDGET, FitPage, FitWidth, MAX_ZOOM, MIN_ZOOM, NextMatch, NextPage,
    PAGE_HORIZONTAL_MARGIN, PAGE_VERTICAL_MARGIN, PreviousMatch, PreviousPage, ResetZoom,
    ToggleSearch, ToggleSidebar, ZOOM_STEP, ZoomIn, ZoomOut,
    view::{
        PdfView, PdfViewEvent, ZoomMode, estimate_page_bytes, idle_prefetch_order,
        pages_to_keep_within_budget, prefetch_admits,
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
            self.invalidate_cache();
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

    /// Drop cached bitmaps so the next render uses the current zoom. Renders in
    /// flight are cancelled by clearing their tasks, and idle prefetch is halted
    /// so it cannot insert pages rendered at the stale scale; `ensure_pages_rendered`
    /// re-arms it at the new zoom.
    fn invalidate_cache(&mut self) {
        if let Some(state) = self.loaded_mut() {
            state.page_cache.clear();
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
                self.page_dpi(index),
                Priority::Foreground,
                window,
                cx,
            );
        }

        let range_center = (visible_start + visible_end) / 2;
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

    fn page_dpi(&self, index: usize) -> f32 {
        let Some(layout) = self.page_layouts.get(index) else {
            return BASE_DPI * self.zoom;
        };
        let Some(state) = self.loaded() else {
            return BASE_DPI * self.zoom;
        };
        let Some(summary_page) = state.summary.pages.get(index) else {
            return BASE_DPI * self.zoom;
        };
        let page_width = summary_page.width as f32;
        if page_width <= 0.0 {
            return BASE_DPI * self.zoom;
        }
        BASE_DPI * (layout.width / page_width)
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
            })
            .ok();
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
                        match view.prefetch_idle_page(index) {
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
    fn prefetch_idle_page(&self, index: usize) -> PrefetchStep {
        let scale = self.page_dpi(index) / 72.0;
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
            self.committed_cache_bytes(),
            page_bytes,
            CACHE_MEMORY_BUDGET,
        ) {
            return PrefetchStep::Stop;
        }
        PrefetchStep::Render(self.page_dpi(index))
    }

    /// Bytes already committed to the cache: resident bitmaps plus the estimated
    /// size of every in-flight render (which will land in the cache soon).
    fn committed_cache_bytes(&self) -> u64 {
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
                let scale = self.page_dpi(index) / 72.0;
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
        self.invalidate_cache();
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
        self.show_sidebar = !self.show_sidebar;
        cx.notify();
    }

    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_open = !self.search_open;
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
        let target_page = state.search.matches[next];
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
}
