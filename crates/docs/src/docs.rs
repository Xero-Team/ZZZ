//! An in-editor viewer for the bundled documentation.
//!
//! The documentation in `docs/src` is embedded into the binary through the
//! `assets` crate, so it can be read offline. `OpenDocs` opens the viewer,
//! and links inside the pages navigate between documents. `SearchDocs` opens
//! a picker over every embedded page.

use std::{path::Path, sync::Arc};

use assets::{all_docs, lookup_docs_text};
use gpui::{
    Action, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, Styled, Task, WeakEntity,
    Window, actions, div, point, prelude::*, px,
};
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownOptions, MarkdownStyle};
use schemars::JsonSchema;
use serde::Deserialize;
use picker::{Picker, PickerDelegate};
use ui::{IconButton, IconName, ListItem, ListItemSpacing, Tooltip, prelude::*};
use util::ResultExt as _;
use workspace::{Item, ModalView, Workspace, item::ItemEvent};
use zzz_actions::{ChangeKeybinding, OpenDocs};

actions!(
    docs,
    [
        /// Opens the documentation search picker.
        SearchDocs,
    ]
);

/// Opens a specific documentation page.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = docs)]
#[serde(deny_unknown_fields)]
pub struct OpenDocsAt {
    pub path: String,
}

const DEFAULT_DOC: &str = "getting-started.md";

pub fn init(cx: &mut App) {
    cx.observe_new(DocumentationView::register).detach();
}

pub struct DocumentationView {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    markdown: Entity<Markdown>,
    scroll_handle: ScrollHandle,
    current: SharedString,
    back: Vec<SharedString>,
    forward: Vec<SharedString>,
}

impl DocumentationView {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _cx: &mut Context<Workspace>,
    ) {
        workspace
            .register_action(|workspace, _: &OpenDocs, window, cx| {
                Self::open_documentation_page(workspace, None, window, cx);
                cx.notify();
            })
            .register_action(|workspace, action: &OpenDocsAt, window, cx| {
                Self::open_documentation_page(
                    workspace,
                    Some(action.path.clone().into()),
                    window,
                    cx,
                );
                cx.notify();
            })
            .register_action(|workspace, _: &SearchDocs, window, cx| {
                Self::open_search(workspace, window, cx);
            });
    }

    fn open_search(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let workspace_handle = workspace.weak_handle();
        workspace.toggle_modal(window, cx, move |window, cx| {
            DocsSearch::new(workspace_handle, window, cx)
        });
    }

    pub fn open_documentation_page(
        workspace: &mut Workspace,
        path: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let path = path.unwrap_or_else(|| SharedString::from(DEFAULT_DOC));

        if let Some(existing) = workspace.item_of_type::<DocumentationView>(cx) {
            workspace.activate_item(&existing, true, true, window, cx);
            existing.update(cx, |this, cx| {
                this.navigate(path, window, cx);
            });
            return;
        }

        let view = cx.new(|cx| Self::new(path, workspace.weak_handle(), window, cx));
        workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
    }

    fn new(
        path: SharedString,
        workspace: WeakEntity<Workspace>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                SharedString::default(),
                None,
                None,
                MarkdownOptions {
                    parse_heading_slugs: true,
                    ..Default::default()
                },
                cx,
            )
        });
        let mut this = Self {
            workspace,
            focus_handle: cx.focus_handle(),
            markdown,
            scroll_handle: ScrollHandle::new(),
            current: SharedString::default(),
            back: Vec::new(),
            forward: Vec::new(),
        };
        this.set_path(path, cx);
        this
    }

    /// Navigates to `path`, recording it in the history.
    fn navigate(&mut self, path: SharedString, _window: &mut Window, cx: &mut Context<Self>) {
        if path == self.current {
            return;
        }
        if !self.current.is_empty() {
            self.back.push(self.current.clone());
        }
        self.forward.clear();
        self.set_path(path, cx);
    }

    fn set_path(&mut self, path: SharedString, cx: &mut Context<Self>) {
        let Some(text) = lookup_docs_text(&path) else {
            return;
        };
        self.current = path;
        self.markdown
            .update(cx, |markdown, cx| markdown.reset(text.into(), cx));
        self.scroll_handle.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.back.pop() else {
            return;
        };
        self.forward.push(self.current.clone());
        self.set_path(previous, cx);
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        let Some(next) = self.forward.pop() else {
            return;
        };
        self.back.push(self.current.clone());
        self.set_path(next, cx);
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1()
            .child(
                IconButton::new("docs-search", IconName::MagnifyingGlass)
                    .tooltip(Tooltip::text(i18n::tr(
                        cx,
                        "docs.search_documentation",
                        "Search Documentation",
                    )))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| {
                                Self::open_search(workspace, window, cx);
                            })
                            .log_err();
                    })),
            )
            .child(
                IconButton::new("docs-back", IconName::ArrowLeft)
                    .disabled(self.back.is_empty())
                    .tooltip(Tooltip::text(i18n::tr(cx, "docs.go_back", "Back")))
                    .on_click(cx.listener(|this, _, _, cx| this.go_back(cx))),
            )
            .child(
                IconButton::new("docs-forward", IconName::ArrowRight)
                    .disabled(self.forward.is_empty())
                    .tooltip(Tooltip::text(i18n::tr(cx, "docs.go_forward", "Forward")))
                    .on_click(cx.listener(|this, _, _, cx| this.go_forward(cx))),
            )
    }
}

impl Render for DocumentationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current.clone();
        let workspace = self.workspace.clone();
        let markdown_style = MarkdownStyle::themed(MarkdownFont::Preview, window, cx);

        let markdown_element = MarkdownElement::new(self.markdown.clone(), markdown_style)
            .scroll_handle(self.scroll_handle.clone())
            .on_url_click(move |url, window, cx| {
                open_doc_url(url, &current, &workspace, window, cx);
            });

        v_flex()
            .key_context("DocumentationView")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(div().p_2().child(self.render_toolbar(cx)))
            .child(
                div()
                    .id("documentation-scroll-container")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .p_4()
                    .child(markdown_element),
            )
    }
}

impl Focusable for DocumentationView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for DocumentationView {}

impl Item for DocumentationView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        SharedString::from(
            self.current
                .strip_suffix(".md")
                .unwrap_or(&self.current)
                .to_string(),
        )
    }

    fn to_item_events(_event: &Self::Event, _f: &mut dyn FnMut(ItemEvent)) {}
}

/// Resolves a link from a documentation page and opens the target.
fn open_doc_url(
    url: SharedString,
    current_path: &SharedString,
    workspace: &WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(action) = url.strip_prefix("zzz://kb/") {
        window.dispatch_action(
            Box::new(ChangeKeybinding {
                action: action.to_string(),
            }),
            cx,
        );
        return;
    }
    if let Some(action) = url.strip_prefix("zzz://action/") {
        window.dispatch_action(
            Box::new(ChangeKeybinding {
                action: action.to_string(),
            }),
            cx,
        );
        return;
    }
    if let Some(path) = url.strip_prefix("zzz://docs/") {
        workspace
            .update(cx, |workspace, cx| {
                DocumentationView::open_documentation_page(
                    workspace,
                    Some(path.to_string().into()),
                    window,
                    cx,
                );
            })
            .log_err();
        return;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        cx.open_url(&url);
        return;
    }
    if url.starts_with("zzz://") {
        window.dispatch_action(
            Box::new(zzz_actions::OpenZZZUrl {
                url: url.to_string(),
            }),
            cx,
        );
        return;
    }

    // Relative link: resolve it against the current document.
    let url = url.split_once('#').map(|(path, _)| path).unwrap_or(&url);
    if url.is_empty() {
        return;
    }
    let base = Path::new(current_path.as_ref()).parent();
    let resolved = match base {
        Some(base) if !url.starts_with('/') => {
            let mut joined = base.join(url);
            joined = normalize_path(&joined);
            joined.to_string_lossy().into_owned()
        }
        _ => url.trim_start_matches('/').to_string(),
    };
    let resolved = if resolved.ends_with(".md") {
        resolved
    } else {
        format!("{resolved}.md")
    };

    workspace
        .update(cx, |workspace, cx| {
            DocumentationView::open_documentation_page(
                workspace,
                Some(resolved.into()),
                window,
                cx,
            );
        })
        .log_err();
}

/// Normalizes `.` and `..` components in a relative path without touching the
/// filesystem.
fn normalize_path(path: &Path) -> std::path::PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            component => components.push(component.as_os_str().to_owned()),
        }
    }
    components.into_iter().collect()
}

// -- Documentation search ---------------------------------------------------

pub struct DocsSearch {
    picker: Entity<Picker<DocsSearchDelegate>>,
}

impl DocsSearch {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = DocsSearchDelegate::new(workspace);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));
        Self { picker }
    }
}

impl Render for DocsSearch {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(rems(34.)).child(self.picker.clone())
    }
}

impl Focusable for DocsSearch {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl EventEmitter<DismissEvent> for DocsSearch {}

impl ModalView for DocsSearch {}

struct DocEntry {
    path: SharedString,
    title: SharedString,
    search_text: String,
}

impl DocEntry {
    fn new(path: SharedString) -> Self {
        let content = lookup_docs_text(&path).unwrap_or_default();
        let title = first_heading(&content)
            .map(SharedString::from)
            .unwrap_or_else(|| humanize_path(&path).into());
        let search_text = format!("{} {}", path, content).to_lowercase();
        Self {
            path,
            title,
            search_text,
        }
    }
}

struct DocsSearchDelegate {
    workspace: WeakEntity<Workspace>,
    entries: Vec<DocEntry>,
    matches: Vec<usize>,
    selected_index: usize,
}

impl DocsSearchDelegate {
    fn new(workspace: WeakEntity<Workspace>) -> Self {
        let entries: Vec<DocEntry> = all_docs()
            .into_iter()
            .filter(|path| path.ends_with(".md"))
            .map(DocEntry::new)
            .collect();
        let matches = (0..entries.len()).collect();
        Self {
            workspace,
            entries,
            matches,
            selected_index: 0,
        }
    }
}

impl PickerDelegate for DocsSearchDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "DocsSearchDelegate"
    }

    fn placeholder_text(&self, _window: &mut Window, cx: &mut App) -> Arc<str> {
        i18n::tr(cx, "docs.search_placeholder", "Search the documentation...").into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            self.matches = (0..self.entries.len()).collect();
        } else {
            self.matches = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.search_text.contains(&query))
                .map(|(ix, _)| ix)
                .collect();
        }
        self.selected_index = 0;
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(ix) = self.matches.get(self.selected_index) {
            let path = self.entries[*ix].path.clone();
            self.workspace
                .update(cx, |workspace, cx| {
                    DocumentationView::open_documentation_page(workspace, Some(path), window, cx);
                })
                .log_err();
        }
        self.dismissed(window, cx);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let entry = self.entries.get(*self.matches.get(ix)?)?;
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(Label::new(entry.title.clone()))
                .end_slot(
                    Label::new(entry.path.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
    }
}

fn first_heading(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|heading| heading.trim().to_string())
    })
}

fn humanize_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_suffix(".md").unwrap_or(name);
    name.replace(['-', '_'], " ")
}
