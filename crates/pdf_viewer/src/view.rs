use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Context as _;
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IsZero, Pixels,
    ScrollHandle, Task, Window,
};
use i18n::tr;
use project::{Project, ProjectPath};
use ui_input::InputField;
use util::rel_path::RelPath;

use crate::{
    FALLBACK_PAGE_HEIGHT, FALLBACK_PAGE_WIDTH, PAGE_GAP, PAGE_VERTICAL_MARGIN, PdfItem,
    document::{PagePreview, PdfDocumentSummary},
    worker::PdfWorker,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ZoomMode {
    /// Explicit zoom factor set by the user.
    Custom,
    FitWidth,
    FitPage,
}

pub(crate) enum LoadState {
    Loading,
    PasswordRequired(Box<PasswordRequiredState>),
    Loaded(Box<LoadedState>),
    Error(gpui::SharedString),
}

pub(crate) struct PasswordRequiredState {
    pub path: PathBuf,
    pub data: Arc<[u8]>,
    pub input: Entity<InputField>,
    pub error: Option<gpui::SharedString>,
    pub retry_task: Option<Task<()>>,
    pub opening: bool,
}

impl PasswordRequiredState {
    fn new(
        path: PathBuf,
        data: Arc<[u8]>,
        error: Option<gpui::SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let input = cx.new(|cx| {
            let placeholder = tr(
                cx,
                "pdf_viewer.password.placeholder",
                "Enter the document password",
            );
            InputField::new(window, cx, &placeholder)
                .label(tr(cx, "pdf_viewer.password.label", "Password"))
                .masked(true)
        });

        Self {
            path,
            data,
            input,
            error,
            retry_task: None,
            opening: false,
        }
    }
}

pub(crate) struct LoadedState {
    pub worker: Arc<PdfWorker>,
    pub summary: Arc<PdfDocumentSummary>,
    /// Rendered page bitmaps keyed by page index.
    pub page_cache: HashMap<usize, PagePreview>,
    /// Pages with an in-flight render, to avoid duplicate work.
    pub rendering: HashMap<usize, Task<()>>,
    pub search: SearchState,
    pub encryption_warning: bool,
    /// Low-priority task that fills the cache outward from the current page
    /// while the user is idle, bounded by [`crate::CACHE_MEMORY_BUDGET`].
    pub idle_prefetch: Option<Task<()>>,
}

#[derive(Default)]
pub(crate) struct SearchState {
    pub query: gpui::SharedString,
    /// (page_index) of each match, in document order.
    pub matches: Vec<usize>,
    pub active_match: Option<usize>,
    /// Cached per-page extracted text, populated lazily during search.
    pub page_text: HashMap<usize, String>,
    pub task: Option<Task<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub top: f32,
}

/// The pages that intersect the viewport plus the single reading-focus page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VisiblePages {
    /// First page (inclusive) overlapping the viewport.
    pub first: usize,
    /// Last page (inclusive) overlapping the viewport.
    pub last: usize,
    /// The page that occupies the most of the viewport. This is what the page
    /// indicator and persistence treat as the "current" page.
    pub focused: usize,
}

/// Compute the vertical `(top, bottom)` span of every page in a continuous
/// layout. Kept as a free function so the layout math can be unit-tested
/// against real page dimensions without constructing a `PdfView`. Must stay in
/// sync with [`PdfView::update_page_layouts`].
pub(crate) fn page_spans(page_heights: impl Iterator<Item = f32>, zoom: f32) -> Vec<(f32, f32)> {
    let mut spans = Vec::new();
    let mut top = PAGE_VERTICAL_MARGIN;
    for page_height in page_heights {
        let height = page_height * zoom;
        spans.push((top, top + height));
        top += height + PAGE_GAP;
    }
    spans
}

/// Resolve which pages are visible and which one is the reading focus, given
/// the viewport edges and per-page spans expressed on a single shared vertical
/// axis. The function is agnostic to the coordinate origin, so the same logic
/// serves both the live scroll handle geometry and synthetic test geometry.
///
/// The focus is the page with the largest visible height; ties resolve to the
/// upper page so that the current page only advances once the next page covers
/// more of the viewport. Returns `None` only when no page overlaps the
/// viewport (e.g. before the document has been laid out).
pub(crate) fn compute_visible_pages(
    viewport_top: f32,
    viewport_bottom: f32,
    spans: &[(f32, f32)],
) -> Option<VisiblePages> {
    let mut first = None;
    let mut last = 0;
    let mut focused = 0;
    let mut best_overlap = 0.0_f32;
    for (index, &(top, bottom)) in spans.iter().enumerate() {
        let overlap = viewport_bottom.min(bottom) - viewport_top.max(top);
        if overlap > 0.0 {
            first.get_or_insert(index);
            last = index;
            if overlap > best_overlap {
                best_overlap = overlap;
                focused = index;
            }
        }
    }
    first.map(|first| VisiblePages {
        first,
        last,
        focused,
    })
}

/// Bytes a rendered page bitmap occupies at the given layout size and scale.
/// Pages render to RGBA (4 bytes/pixel) at `width * scale` by `height * scale`
/// device pixels, where `scale = dpi / 72`. Used to keep the page cache within
/// a memory budget without having to wait for each render to complete.
pub(crate) fn estimate_page_bytes(layout_width: f32, layout_height: f32, scale: f32) -> u64 {
    let device_width = (layout_width * scale).max(0.0) as u64;
    let device_height = (layout_height * scale).max(0.0) as u64;
    device_width.saturating_mul(device_height).saturating_mul(4)
}

/// Choose which cached pages to keep so their combined bytes stay within
/// `budget`, preferring pages nearest `center`. `pages` is the set of currently
/// cached `(index, bytes)` entries. Returns the indices to retain.
///
/// The nearest page is always kept even if a single page exceeds the budget, so
/// the page the user is reading is never evicted out from under them.
pub(crate) fn pages_to_keep_within_budget(
    mut pages: Vec<(usize, u64)>,
    center: usize,
    budget: u64,
) -> Vec<usize> {
    pages.sort_by_key(|&(index, _)| center.abs_diff(index));
    let mut kept = Vec::new();
    let mut used: u64 = 0;
    for (position, (index, bytes)) in pages.into_iter().enumerate() {
        // Always keep the closest page; otherwise keep while within budget.
        if position == 0 || used.saturating_add(bytes) <= budget {
            used = used.saturating_add(bytes);
            kept.push(index);
        }
    }
    kept
}

/// Page indices to prefetch when idle, ordered outward from `center` (the page
/// just below `center` first, then just above, alternating). Excludes `center`
/// itself and stops at the document bounds. The caller stops consuming this
/// list once the memory budget is reached, so it is safe to request the full
/// remainder of the document.
pub(crate) fn idle_prefetch_order(center: usize, page_count: usize) -> Vec<usize> {
    let mut order = Vec::new();
    let mut distance = 1usize;
    while order.len() < page_count.saturating_sub(1) {
        let mut progressed = false;
        if let Some(below) = center.checked_add(distance).filter(|&i| i < page_count) {
            order.push(below);
            progressed = true;
        }
        if let Some(above) = center.checked_sub(distance) {
            order.push(above);
            progressed = true;
        }
        if !progressed {
            break;
        }
        distance += 1;
    }
    order
}

/// Whether idle prefetch should admit one more page of `page_bytes`, given the
/// bytes already committed (resident bitmaps **plus** renders in flight) and
/// the budget. The in-flight term is the crux: idle prefetch submits faster
/// than the worker pool renders, so counting only resident bytes lets it queue
/// the whole document before the first bitmaps land. Counting committed bytes
/// caps the number of outstanding renders, which is what actually bounds peak
/// memory.
pub(crate) fn prefetch_admits(committed_bytes: u64, page_bytes: u64, budget: u64) -> bool {
    committed_bytes.saturating_add(page_bytes) <= budget
}

pub struct PdfView {
    pub(crate) pdf_item: Entity<PdfItem>,
    pub(crate) project: Entity<Project>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) load_state: LoadState,
    pub(crate) current_page: usize,
    pub(crate) zoom: f32,
    pub(crate) zoom_mode: ZoomMode,
    pub(crate) show_sidebar: bool,
    pub(crate) search_open: bool,
    pub(crate) container_bounds: Option<Bounds<Pixels>>,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) page_layouts: Vec<PageLayout>,
    pub(crate) document_height: f32,
    pub(crate) pending_scroll_to_page: Option<usize>,
    pub(crate) search_editor: Entity<editor::Editor>,
    _load_task: Task<()>,
    _search_subscription: gpui::Subscription,
}

pub enum PdfViewEvent {
    TitleChanged,
}

impl EventEmitter<PdfViewEvent> for PdfView {}
impl EventEmitter<()> for PdfView {}

impl Focusable for PdfView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn is_wrong_password(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<zpdf::Error>(),
            Some(zpdf::Error::WrongPassword)
        )
    })
}

impl PdfView {
    pub fn new(
        pdf_item: Entity<PdfItem>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_editor = cx.new(|cx| {
            let mut editor = editor::Editor::single_line(window, cx);
            let placeholder = tr(cx, "pdf_viewer.search.placeholder", "Search in document");
            editor.set_placeholder_text(&placeholder, window, cx);
            editor
        });
        let search_subscription =
            cx.subscribe_in(&search_editor, window, Self::on_search_editor_event);

        let load_task = Self::start_load(&pdf_item, &project, window, cx);

        Self {
            pdf_item,
            project,
            focus_handle: cx.focus_handle(),
            load_state: LoadState::Loading,
            current_page: 0,
            zoom: 1.0,
            zoom_mode: ZoomMode::FitWidth,
            show_sidebar: false,
            search_open: false,
            container_bounds: None,
            scroll_handle: ScrollHandle::new(),
            page_layouts: Vec::new(),
            document_height: 0.0,
            pending_scroll_to_page: None,
            search_editor,
            _load_task: load_task,
            _search_subscription: search_subscription,
        }
    }

    fn start_load(
        pdf_item: &Entity<PdfItem>,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let relative_path = pdf_item.read(cx).path.clone();
        let worktree_id = pdf_item.read(cx).worktree_id;
        let project = project.clone();

        cx.spawn_in(window, async move |this, cx| {
            let abs_path = cx
                .update(|_window, cx| {
                    project
                        .read(cx)
                        .worktree_for_id(worktree_id, cx)
                        .and_then(|worktree| {
                            worktree
                                .read(cx)
                                .as_local()
                                .map(|local| local.abs_path().join(relative_path.as_std_path()))
                        })
                })
                .ok()
                .flatten();

            let Some(abs_path) = abs_path else {
                Self::set_error(&this, "Could not resolve PDF path", cx);
                return;
            };

            let load_result = cx
                .background_spawn(async move {
                    let data = std::fs::read(&abs_path)
                        .with_context(|| format!("reading {abs_path:?}"))?;
                    anyhow::Ok((abs_path, Arc::<[u8]>::from(data)))
                })
                .await;

            let (abs_path, data) = match load_result {
                Ok(loaded) => loaded,
                Err(error) => {
                    Self::set_error(&this, error.to_string(), cx);
                    return;
                }
            };

            match PdfWorker::open(abs_path.clone(), data.clone(), Vec::new()).await {
                Ok(worker) => {
                    this.update_in(cx, |view, window, cx| {
                        view.set_loaded(Arc::new(worker), window, cx);
                    })
                    .ok();
                }
                Err(error) if is_wrong_password(&error) => {
                    Self::set_password_required(&this, abs_path, data, None, cx);
                }
                Err(error) => Self::set_error(&this, error.to_string(), cx),
            }
        })
    }

    fn set_error(
        this: &gpui::WeakEntity<Self>,
        message: impl Into<gpui::SharedString>,
        cx: &mut gpui::AsyncWindowContext,
    ) {
        let message = message.into();
        this.update(cx, |view, cx| {
            view.load_state = LoadState::Error(message);
            cx.notify();
        })
        .ok();
    }

    fn set_password_required(
        this: &gpui::WeakEntity<Self>,
        path: PathBuf,
        data: Arc<[u8]>,
        error: Option<gpui::SharedString>,
        cx: &mut gpui::AsyncWindowContext,
    ) {
        this.update_in(cx, |view, window, cx| {
            view.load_state = LoadState::PasswordRequired(Box::new(PasswordRequiredState::new(
                path, data, error, window, cx,
            )));
            view.focus_password_input(window, cx);
            cx.notify();
        })
        .ok();
    }

    fn set_loaded(&mut self, worker: Arc<PdfWorker>, window: &mut Window, cx: &mut Context<Self>) {
        let summary = worker.summary().clone();
        self.load_state = LoadState::Loaded(Box::new(LoadedState {
            worker,
            summary,
            page_cache: HashMap::default(),
            rendering: HashMap::default(),
            search: SearchState::default(),
            encryption_warning: false,
            idle_prefetch: None,
        }));
        self.current_page = self.current_page.min(self.page_count().saturating_sub(1));
        self.update_page_layouts();
        self.ensure_pages_rendered(window, cx);
        cx.emit(PdfViewEvent::TitleChanged);
        cx.notify();
    }

    fn focus_password_input(&self, window: &mut Window, cx: &mut App) {
        if let LoadState::PasswordRequired(state) = &self.load_state {
            let focus_handle = state.input.focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub(crate) fn submit_password(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let LoadState::PasswordRequired(state) = &mut self.load_state else {
            return;
        };
        if state.opening {
            return;
        }

        let input = state.input.clone();
        let password = input.read(cx).text(cx);
        let empty_password_error = tr(
            cx,
            "pdf_viewer.password.empty_error",
            "Enter a password to unlock this PDF.",
        );
        let prepared_password = if password.is_empty() {
            state.error = Some(empty_password_error.into());
            None
        } else {
            input.update(cx, |input, cx| input.clear(window, cx));
            state.error = None;
            state.opening = true;
            Some((
                state.path.clone(),
                state.data.clone(),
                password.into_bytes(),
            ))
        };
        let Some((path, data, mut password)) = prepared_password else {
            self.focus_password_input(window, cx);
            cx.notify();
            return;
        };

        let task = cx.spawn_in(window, async move |this, cx| {
            let result = PdfWorker::open(path, data, password.clone()).await;
            password.fill(0);

            this.update_in(cx, |view, window, cx| match result {
                Ok(worker) => {
                    view.set_loaded(Arc::new(worker), window, cx);
                }
                Err(error) if is_wrong_password(&error) => {
                    if let LoadState::PasswordRequired(state) = &mut view.load_state {
                        state.opening = false;
                        state.retry_task = None;
                        state.error = Some(
                            tr(
                                cx,
                                "pdf_viewer.password.invalid_error",
                                "Incorrect password. Try again.",
                            )
                            .into(),
                        );
                    }
                    view.focus_password_input(window, cx);
                    cx.notify();
                }
                Err(error) => {
                    view.load_state = LoadState::Error(error.to_string().into());
                    cx.notify();
                }
            })
            .ok();
        });
        state.retry_task = Some(task);
        cx.notify();
    }

    pub(crate) fn loaded(&self) -> Option<&LoadedState> {
        match &self.load_state {
            LoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn loaded_mut(&mut self) -> Option<&mut LoadedState> {
        match &mut self.load_state {
            LoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    pub fn page_count(&self) -> usize {
        self.loaded()
            .map(|state| state.summary.page_count)
            .unwrap_or(0)
    }

    pub fn current_page(&self) -> usize {
        self.current_page
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub(crate) fn current_page_layout(&self) -> Option<PageLayout> {
        self.page_layouts.get(self.current_page).copied()
    }

    /// Resolve which pages overlap the viewport and which one holds the reading
    /// focus, using the live geometry tracked by the scroll handle.
    ///
    /// GPUI records each child's bounds in a *static* layout space that does not
    /// move as the user scrolls; the viewport's position within that space is
    /// its own bounds shifted by the (negative) scroll offset. The previous
    /// implementation subtracted the offset from both sides, which cancelled out
    /// and made the result independent of the scroll position. Here the offset
    /// is applied only to the viewport, so the focused page tracks scrolling.
    pub(crate) fn visible_pages(&self) -> Option<VisiblePages> {
        let page_count = self.page_count();
        if page_count == 0 {
            return None;
        }
        let viewport = self.scroll_handle.bounds();
        if viewport.size.height.is_zero() {
            return None;
        }
        let offset = self.scroll_handle.offset();
        let viewport_top = f32::from(viewport.top() - offset.y);
        let viewport_bottom = f32::from(viewport.bottom() - offset.y);

        // The scroll container wraps the page children between a leading and a
        // trailing spacer, so the page at `index` is child `index + 1`.
        let mut spans = Vec::with_capacity(page_count);
        for index in 0..page_count {
            let bounds = self.scroll_handle.bounds_for_item(index + 1)?;
            spans.push((f32::from(bounds.top()), f32::from(bounds.bottom())));
        }
        compute_visible_pages(viewport_top, viewport_bottom, &spans)
    }

    pub(crate) fn visible_page_range(&self) -> Option<(usize, usize)> {
        self.visible_pages().map(|pages| (pages.first, pages.last))
    }

    pub(crate) fn update_page_layouts(&mut self) {
        let Some(state) = self.loaded() else {
            self.page_layouts.clear();
            self.document_height = 0.0;
            return;
        };

        // Keep `page_layouts` index-aligned with page indices (and therefore
        // with the scroll children) so `current_page` can index it directly.
        // Degenerate pages fall back to a nominal size rather than being
        // dropped, which would shift every later page's index.
        let zoom = self.zoom;
        let sizes: Vec<(f32, f32)> = state
            .summary
            .pages
            .iter()
            .map(|page| {
                let width = page.width as f32;
                let height = page.height as f32;
                if width > 0.0 && height > 0.0 {
                    (width * zoom, height * zoom)
                } else {
                    (FALLBACK_PAGE_WIDTH * zoom, FALLBACK_PAGE_HEIGHT * zoom)
                }
            })
            .collect();

        let spans = page_spans(sizes.iter().map(|&(_, height)| height), 1.0);
        let layouts: Vec<PageLayout> = sizes
            .iter()
            .zip(&spans)
            .map(|(&(width, height), &(top, _))| PageLayout { width, height, top })
            .collect();

        self.document_height = match spans.last() {
            Some(&(_, bottom)) => bottom + PAGE_VERTICAL_MARGIN,
            None => 0.0,
        };
        self.page_layouts = layouts;
    }

    pub(crate) fn sync_current_page_with_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(pages) = self.visible_pages() else {
            return;
        };
        if pages.focused != self.current_page {
            self.current_page = pages.focused;
            cx.emit(PdfViewEvent::TitleChanged);
            cx.notify();
        }
    }

    pub(crate) fn scroll_to_current_page(&mut self, cx: &mut Context<Self>) {
        if self.page_layouts.get(self.current_page).is_none() {
            self.pending_scroll_to_page = Some(self.current_page);
            return;
        }
        self.pending_scroll_to_page = None;
        self.scroll_handle.scroll_to_top_of_item(self.current_page);
        cx.notify();
    }

    pub(crate) fn flush_pending_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(target_page) = self.pending_scroll_to_page.take() else {
            return;
        };
        if self.page_layouts.get(target_page).is_none() {
            self.pending_scroll_to_page = Some(target_page);
            return;
        }
        self.scroll_handle.scroll_to_top_of_item(target_page);
        cx.notify();
    }

    pub(crate) fn file_name(&self, cx: &App) -> String {
        self.pdf_item
            .read(cx)
            .path
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| "PDF".to_owned())
    }

    pub(crate) fn abs_path(&self, cx: &App) -> Option<PathBuf> {
        let item = self.pdf_item.read(cx);
        let worktree = self
            .project
            .read(cx)
            .worktree_for_id(item.worktree_id, cx)?;
        let local = worktree.read(cx).as_local()?;
        Some(local.abs_path().join(item.path.as_std_path()))
    }

    pub(crate) fn project_path(&self, cx: &App) -> ProjectPath {
        self.pdf_item.read(cx).project_path()
    }

    pub(crate) fn relative_path(&self, cx: &App) -> Arc<RelPath> {
        self.pdf_item.read(cx).path.clone()
    }

    /// A multi-line description of the document used for the tab tooltip:
    /// path, optional title/author, PDF version and page count.
    pub(crate) fn metadata_tooltip(&self, cx: &App) -> String {
        let path_style = self.project.read(cx).path_style(cx);
        let mut lines = vec![self.relative_path(cx).display(path_style).into_owned()];
        if let Some(summary) = self.loaded().map(|state| &state.summary) {
            if let Some(title) = &summary.title {
                lines.push(format!("Title: {title}"));
            }
            if let Some(author) = &summary.author {
                lines.push(format!("Author: {author}"));
            }
            if summary.security == crate::document::PdfSecurity::Encrypted {
                lines.push("Encrypted document".to_owned());
            }
            lines.push(format!(
                "PDF {}.{} · {} page{}",
                summary.version.0,
                summary.version.1,
                summary.page_count,
                if summary.page_count == 1 { "" } else { "s" },
            ));
        }
        lines.join("\n")
    }

    pub(crate) fn zoom_mode_key(&self) -> &'static str {
        match self.zoom_mode {
            ZoomMode::Custom => "custom",
            ZoomMode::FitWidth => "fit_width",
            ZoomMode::FitPage => "fit_page",
        }
    }

    /// Apply persisted reading state. Renders are scheduled lazily once the
    /// document finishes loading and the container size is known.
    pub(crate) fn restore_state(&mut self, page: usize, zoom_mode: ZoomMode, zoom: f32) {
        self.current_page = page;
        self.zoom_mode = zoom_mode;
        self.zoom = zoom.clamp(crate::MIN_ZOOM, crate::MAX_ZOOM);
    }

    pub(crate) fn encryption_warning(&self) -> bool {
        self.loaded().is_some_and(|state| {
            state.encryption_warning
                || state.summary.security == crate::document::PdfSecurity::Encrypted
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_visible_pages, estimate_page_bytes, idle_prefetch_order, is_wrong_password,
        page_spans, pages_to_keep_within_budget, prefetch_admits,
    };
    use crate::{PAGE_GAP, PAGE_VERTICAL_MARGIN};

    /// A viewport whose top edge sits `content_top` points into the content,
    /// matching how `visible_pages` derives the viewport from `-offset.y`.
    fn visible_at(
        content_top: f32,
        viewport_height: f32,
        spans: &[(f32, f32)],
    ) -> super::VisiblePages {
        compute_visible_pages(content_top, content_top + viewport_height, spans)
            .expect("a non-empty document always has a visible page")
    }

    #[test]
    fn page_spans_stack_with_margins_and_gaps() {
        // Three 100pt-tall pages at zoom 1.0.
        let spans = page_spans([100.0, 100.0, 100.0].into_iter(), 1.0);
        assert_eq!(
            spans[0],
            (PAGE_VERTICAL_MARGIN, PAGE_VERTICAL_MARGIN + 100.0)
        );
        // Each later page starts a gap below the previous page's bottom.
        assert_eq!(spans[1].0, spans[0].1 + PAGE_GAP);
        assert_eq!(spans[2].0, spans[1].1 + PAGE_GAP);
    }

    #[test]
    fn wrong_password_detection_survives_anyhow_context() {
        let error = anyhow::Error::new(zpdf::Error::WrongPassword)
            .context("opening password-protected PDF");
        assert!(is_wrong_password(&error));
        assert!(!is_wrong_password(&anyhow::anyhow!("some other failure")));
    }

    #[test]
    fn focus_tracks_the_page_covering_most_of_the_viewport() {
        // 500pt pages so a 400pt viewport never spans more than two of them.
        let spans = page_spans(std::iter::repeat(500.0).take(10), 1.0);
        let viewport = 400.0;

        // Parked at the top: page 0 is focused.
        assert_eq!(visible_at(spans[0].0, viewport, &spans).focused, 0);

        // Scrolled so most of the viewport shows page 3.
        let near_page_3 = spans[3].0 + 50.0;
        let pages = visible_at(near_page_3, viewport, &spans);
        assert_eq!(pages.focused, 3);
        assert!(pages.first <= 3 && pages.last >= 3);

        // Straddling the 4/5 boundary with page 5 covering more flips focus to 5.
        let straddle = spans[4].1 - 100.0;
        let pages = visible_at(straddle, viewport, &spans);
        assert_eq!(pages.first, 4);
        assert_eq!(pages.last, 5);
        assert_eq!(pages.focused, 5);
    }

    #[test]
    fn focus_advances_monotonically_while_scrolling_down() {
        let spans = page_spans(std::iter::repeat(300.0).take(50), 1.0);
        let viewport = 350.0;
        let document_bottom = spans.last().expect("non-empty").1;

        let mut last_focus = 0;
        let mut content_top = spans[0].0;
        while content_top + viewport < document_bottom {
            let focus = visible_at(content_top, viewport, &spans).focused;
            assert!(
                focus >= last_focus,
                "focus regressed from {last_focus} to {focus} at content_top {content_top}",
            );
            last_focus = focus;
            content_top += 60.0;
        }
        // Reaching the end of the document focuses one of the final pages.
        assert!(
            last_focus >= 48,
            "expected to reach the document end, got {last_focus}"
        );
    }

    #[test]
    fn no_overlap_before_layout_returns_none() {
        assert_eq!(compute_visible_pages(0.0, 100.0, &[]), None);
        // A viewport entirely above the first page (negative scroll rubber-band)
        // still reports the nearest page rather than nothing.
        let spans = page_spans([100.0].into_iter(), 1.0);
        assert_eq!(compute_visible_pages(-500.0, -400.0, &spans), None);
    }

    #[test]
    fn idle_prefetch_expands_outward_alternating() {
        // From the middle of a 7-page doc, expand below-first then above,
        // growing the distance each round, never repeating or including center.
        assert_eq!(idle_prefetch_order(3, 7), vec![4, 2, 5, 1, 6, 0]);
    }

    #[test]
    fn idle_prefetch_handles_edges_and_tiny_documents() {
        // At the first page, only pages below exist.
        assert_eq!(idle_prefetch_order(0, 4), vec![1, 2, 3]);
        // At the last page, only pages above exist.
        assert_eq!(idle_prefetch_order(3, 4), vec![2, 1, 0]);
        // Single-page (and empty) documents have nothing to prefetch.
        assert!(idle_prefetch_order(0, 1).is_empty());
        assert!(idle_prefetch_order(0, 0).is_empty());
    }

    #[test]
    fn idle_prefetch_covers_every_other_page_exactly_once() {
        let order = idle_prefetch_order(5, 20);
        let mut seen = order.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), order.len(), "no page is prefetched twice");
        assert_eq!(order.len(), 19, "every page except the center is covered");
        assert!(!order.contains(&5), "the center page is excluded");
    }

    #[test]
    fn estimate_page_bytes_matches_rgba_device_pixels() {
        // 612x792pt at scale 2.0 -> 1224x1584 px, 4 bytes each.
        let bytes = estimate_page_bytes(612.0, 792.0, 2.0);
        assert_eq!(bytes, 1224 * 1584 * 4);
        // Degenerate sizes never panic or wrap.
        assert_eq!(estimate_page_bytes(0.0, 100.0, 2.0), 0);
        assert_eq!(estimate_page_bytes(-5.0, 100.0, 2.0), 0);
    }

    #[test]
    fn budget_keeps_pages_nearest_the_center() {
        // Six equal 100-byte pages, budget for three.
        let pages = (0..6).map(|index| (index, 100)).collect::<Vec<_>>();
        let mut kept = pages_to_keep_within_budget(pages, 4, 300);
        kept.sort_unstable();
        assert_eq!(kept, vec![3, 4, 5], "keeps the pages closest to page 4");
    }

    #[test]
    fn budget_always_keeps_the_current_page_even_when_oversized() {
        // The center page alone exceeds the budget; it must still be kept so the
        // page being read is never evicted.
        let pages = vec![(10, 5_000), (11, 100), (9, 100)];
        let kept = pages_to_keep_within_budget(pages, 10, 1_000);
        assert_eq!(
            kept,
            vec![10],
            "the oversized center page is retained alone"
        );
    }

    #[test]
    fn budget_keeps_everything_when_it_all_fits() {
        let pages = vec![(0, 50), (1, 50), (2, 50)];
        let mut kept = pages_to_keep_within_budget(pages, 1, 1_000);
        kept.sort_unstable();
        assert_eq!(kept, vec![0, 1, 2]);
    }

    #[test]
    fn prefetch_counts_in_flight_so_it_cannot_overcommit() {
        // The leak: idle prefetch submits faster than renders complete. Model a
        // worker that has accepted many renders but completed none yet, so the
        // resident cache is still empty while bytes are committed in flight.
        const PAGE: u64 = 8 * 1024 * 1024; // ~8 MB per page
        let budget = 64 * PAGE; // room for 64 pages

        // Counting only *resident* bytes (the bug): with nothing resident yet,
        // every page is admitted, so the loop would queue far past the budget.
        let resident_only = 0;
        let admitted_with_bug = (0..1000)
            .take_while(|_| prefetch_admits(resident_only, PAGE, budget))
            .count();
        assert!(
            admitted_with_bug >= 1000,
            "resident-only accounting never stops — this is the 17 GB leak",
        );

        // Counting *committed* bytes (the fix): each admitted page adds to the
        // outstanding total, so submission halts at the budget regardless of how
        // few have actually finished rendering.
        let mut committed = 0u64;
        let mut admitted = 0;
        for _ in 0..1000 {
            if !prefetch_admits(committed, PAGE, budget) {
                break;
            }
            committed += PAGE;
            admitted += 1;
        }
        assert_eq!(
            admitted, 64,
            "committed accounting caps outstanding renders"
        );
        assert!(committed <= budget);
    }

    /// Drive focus detection over the real 302-page scan
    /// (`tests/信息安全技术扫描件.pdf`). The fixture is ~150 MB and is not
    /// checked into git, so the test no-ops when it is missing rather than
    /// failing on a clean checkout.
    #[test]
    fn focus_tracking_over_real_long_document() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("信息安全技术扫描件.pdf");
        let Ok(data) = std::fs::read(&path) else {
            eprintln!("skipping: fixture {} not present", path.display());
            return;
        };

        let document = crate::document::LoadedPdfDocument::open(path, data.into())
            .expect("real scan should parse");
        let summary = &document.summary;
        assert!(
            summary.page_count > 100,
            "expected a long document, got {} pages",
            summary.page_count,
        );

        // Mirror `update_page_layouts`: 1:1 spans for every page at a fit-width
        // zoom against an 800pt-wide viewport.
        let zoom = 800.0 / summary.pages[0].width as f32;
        let heights = summary
            .pages
            .iter()
            .map(|page| page.height as f32)
            .collect::<Vec<_>>();
        let spans = page_spans(heights.iter().copied(), zoom);
        assert_eq!(spans.len(), summary.page_count);

        let viewport = 900.0;
        let document_bottom = spans.last().expect("non-empty").1;

        // Parking the viewport at each page's top must focus that page, and
        // focus must never run backwards while scrolling down.
        let mut previous_focus = 0;
        for target in 0..summary.page_count {
            let pages = visible_at(spans[target].0, viewport, &spans);
            assert_eq!(
                pages.focused, target,
                "scrolling to the top of page {target} should focus it",
            );
            assert!(pages.first <= target && target <= pages.last);
            assert!(pages.focused >= previous_focus);
            previous_focus = pages.focused;
        }

        // A fine-grained downward sweep stays monotonic and reaches the end.
        let mut content_top = spans[0].0;
        let mut last_focus = 0;
        let step = (viewport / 4.0).max(1.0);
        while content_top + viewport < document_bottom {
            let focus = visible_at(content_top, viewport, &spans).focused;
            assert!(focus >= last_focus, "focus regressed at {content_top}");
            last_focus = focus;
            content_top += step;
        }
        assert!(
            last_focus >= summary.page_count - 2,
            "sweep should reach the final pages, stopped at {last_focus}",
        );
    }

    /// Simulate idle prefetch filling the cache outward from a mid-document page
    /// on the real 302-page scan, and confirm the memory budget bounds the
    /// resident window: prefetch stops well before the whole document is
    /// resident, and the kept set stays within budget and contiguous-ish around
    /// the anchor. Skips when the fixture is absent.
    #[test]
    fn idle_prefetch_stays_within_budget_on_real_document() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("信息安全技术扫描件.pdf");
        let Ok(data) = std::fs::read(&path) else {
            eprintln!("skipping: fixture {} not present", path.display());
            return;
        };
        let document = crate::document::LoadedPdfDocument::open(path, data.into())
            .expect("real scan should parse");
        let summary = &document.summary;

        let zoom = 800.0 / summary.pages[0].width as f32;
        let scale = crate::BASE_DPI * zoom / 72.0;
        let bytes_for = |index: usize| {
            let page = &summary.pages[index];
            estimate_page_bytes(page.width as f32 * zoom, page.height as f32 * zoom, scale)
        };

        let budget = crate::CACHE_MEMORY_BUDGET;
        let center = summary.page_count / 2;

        // Walk the prefetch order through the same predicate the runtime uses,
        // accumulating committed bytes until it refuses the next page — exactly
        // as `prefetch_idle_page` does at runtime.
        let mut resident = vec![center];
        let mut committed = bytes_for(center);
        for index in idle_prefetch_order(center, summary.page_count) {
            if !prefetch_admits(committed, bytes_for(index), budget) {
                break;
            }
            committed = committed.saturating_add(bytes_for(index));
            resident.push(index);
        }

        assert!(
            committed <= budget,
            "committed bytes {committed} exceed budget {budget}",
        );
        assert!(
            resident.len() < summary.page_count,
            "budget should prevent the entire {}-page document from going resident, got {}",
            summary.page_count,
            resident.len(),
        );
        assert!(
            resident.len() > 1,
            "budget should admit more than just the anchor page",
        );

        // The retained set the eviction pass would keep matches what prefetch
        // made resident (nothing over-budget slips through).
        let cached: Vec<(usize, u64)> = resident.iter().map(|&i| (i, bytes_for(i))).collect();
        let kept = pages_to_keep_within_budget(cached, center, budget);
        assert_eq!(
            kept.len(),
            resident.len(),
            "eviction keeps exactly the pages prefetch made resident",
        );
    }
}
