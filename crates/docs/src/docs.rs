//! An in-editor viewer for the bundled documentation.
//!
//! The documentation in `docs/src` is embedded into the binary through the
//! `assets` crate, so it can be read offline. `OpenDocs` opens the viewer,
//! and links inside the pages navigate between documents. `SearchDocs` opens
//! a picker over every embedded page.

use std::sync::Arc;

use assets::{all_docs, lookup_docs_text};
use gpui::{
    Action, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, Styled, Task, WeakEntity, Window, actions,
    div, point, prelude::*, px,
};
use language::LanguageRegistry;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownOptions, MarkdownStyle};
use picker::{Picker, PickerDelegate};
use schemars::JsonSchema;
use serde::Deserialize;
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
    /// A `#fragment` to scroll to once the current page finishes parsing.
    pending_fragment: Option<SharedString>,
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
                    Some(DocTarget::from_href(&action.path)),
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
        target: Option<DocTarget>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let target = target.unwrap_or_else(|| DocTarget::page(DEFAULT_DOC));

        if let Some(existing) = workspace.item_of_type::<DocumentationView>(cx) {
            workspace.activate_item(&existing, true, true, window, cx);
            existing.update(cx, |this, cx| {
                this.navigate(target, cx);
            });
            return;
        }

        let language_registry = workspace.project().read(cx).languages().clone();
        let view = cx.new(|cx| Self::new(target, workspace.weak_handle(), language_registry, cx));
        workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
    }

    fn new(
        target: DocTarget,
        workspace: WeakEntity<Workspace>,
        language_registry: Arc<LanguageRegistry>,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                SharedString::default(),
                Some(language_registry),
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
            pending_fragment: None,
        };
        cx.observe(&this.markdown, |this, _markdown, cx| {
            this.consume_pending_fragment(cx);
        })
        .detach();
        this.set_path(&target, cx);
        this
    }

    /// Navigates to `target`, recording it in the history.
    fn navigate(&mut self, target: DocTarget, cx: &mut Context<Self>) {
        if target.path == self.current && target.fragment.is_none() {
            return;
        }
        if !self.current.is_empty() && target.path != self.current {
            self.back.push(self.current.clone());
        }
        self.forward.clear();
        self.set_path(&target, cx);
    }

    fn set_path(&mut self, target: &DocTarget, cx: &mut Context<Self>) {
        let Some(raw) = lookup_docs_text(&target.path) else {
            return;
        };
        let text = strip_front_matter(&raw);
        self.current = target.path.clone();
        self.pending_fragment = target.fragment.clone();
        self.markdown
            .update(cx, |markdown, cx| markdown.reset(text.into(), cx));
        self.scroll_handle.set_offset(point(px(0.), px(0.)));
        // The document may already be parsed if the source is unchanged, in
        // which case the observe callback above will not fire.
        self.consume_pending_fragment(cx);
        cx.notify();
    }

    /// Scrolls to the pending `#fragment` if it resolves in the parsed page.
    /// Keeps the fragment pending while the page is still parsing so it can be
    /// retried once parsing completes.
    fn consume_pending_fragment(&mut self, cx: &mut Context<Self>) {
        let Some(fragment) = self.pending_fragment.clone() else {
            return;
        };
        let resolved = self
            .markdown
            .update(cx, |markdown, cx| markdown.scroll_to_heading(&fragment, cx));
        if resolved.is_some() {
            self.pending_fragment = None;
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.back.pop() else {
            return;
        };
        self.forward.push(self.current.clone());
        self.set_path(&DocTarget::page(previous), cx);
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        let Some(next) = self.forward.pop() else {
            return;
        };
        self.back.push(self.current.clone());
        self.set_path(&DocTarget::page(next), cx);
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
                handle_doc_link(url, &current, &workspace, window, cx);
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

/// A documentation page plus an optional `#fragment` to scroll to.
#[derive(Clone, PartialEq, Eq)]
pub struct DocTarget {
    path: SharedString,
    fragment: Option<SharedString>,
}

impl DocTarget {
    fn page(path: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            fragment: None,
        }
    }

    /// Parses a `path` or `path#fragment` string into a target.
    fn from_href(href: impl AsRef<str>) -> Self {
        let href = href.as_ref();
        match href.split_once('#') {
            Some((path, fragment)) if !fragment.is_empty() => Self {
                path: path.into(),
                fragment: Some(fragment.into()),
            },
            _ => Self::page(href),
        }
    }
}

/// A parsed documentation link, ready to be dispatched.
enum DocLink {
    /// An action name whose keybinding should be shown.
    Keybinding(SharedString),
    /// Another page in the bundled documentation.
    Docs(DocTarget),
    /// An external URL to open in the system browser.
    External(SharedString),
    /// A `zzz://` URL handled elsewhere in the application.
    AppUrl(SharedString),
}

/// Parses a link from a documentation page. Returns `None` when the link
/// points outside the bundled documentation.
fn resolve_doc_link(current_path: &str, href: &str) -> Option<DocLink> {
    if let Some(action) = href.strip_prefix("zzz://kb/") {
        return Some(DocLink::Keybinding(action.to_string().into()));
    }
    if let Some(action) = href.strip_prefix("zzz://action/") {
        return Some(DocLink::Keybinding(action.to_string().into()));
    }
    if let Some(path) = href.strip_prefix("zzz://docs/") {
        return Some(DocLink::Docs(DocTarget::from_href(path)));
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(DocLink::External(href.to_string().into()));
    }
    if href.starts_with("zzz://") {
        return Some(DocLink::AppUrl(href.to_string().into()));
    }

    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) if !fragment.is_empty() => (path, Some(fragment)),
        _ => (href, None),
    };

    // A bare `#fragment` scrolls within the current page.
    if path.is_empty() {
        let fragment = fragment?;
        let mut target = DocTarget::page(current_path);
        target.fragment = Some(fragment.into());
        return Some(DocLink::Docs(target));
    }

    let resolved = resolve_relative_path(current_path, path)?;
    let mut target = DocTarget::page(resolved);
    target.fragment = fragment.map(SharedString::from);
    Some(DocLink::Docs(target))
}

/// Resolves `path` relative to the directory of `current_path`, treating the
/// documentation root as a virtual boundary. Returns `None` if the link
/// escapes the root or is otherwise unusable.
fn resolve_relative_path(current_path: &str, path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    let base = match current_path.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    };

    let mut components: Vec<&str> = base
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();

    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                // Never step above the documentation root.
                components.pop()?;
            }
            component => components.push(component),
        }
    }

    if components.is_empty() {
        return None;
    }

    let mut resolved = components.join("/");
    if !resolved.ends_with(".md") {
        resolved.push_str(".md");
    }
    Some(resolved)
}

/// Handles a clicked documentation link without re-entering the workspace
/// while it is being updated.
fn handle_doc_link(
    url: SharedString,
    current_path: &SharedString,
    workspace: &WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(link) = resolve_doc_link(current_path.as_ref(), url.as_ref()) else {
        return;
    };

    match link {
        DocLink::Keybinding(action) => {
            window.dispatch_action(
                Box::new(ChangeKeybinding {
                    action: action.into(),
                }),
                cx,
            );
        }
        DocLink::External(url) => cx.open_url(&url),
        DocLink::AppUrl(url) => {
            window.dispatch_action(Box::new(zzz_actions::OpenZZZUrl { url: url.into() }), cx);
        }
        DocLink::Docs(target) => {
            let workspace = workspace.clone();
            window.defer(cx, move |window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        DocumentationView::open_documentation_page(
                            workspace,
                            Some(target),
                            window,
                            cx,
                        );
                    })
                    .log_err();
            });
        }
    }
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
        let title =
            first_heading(&content).map_or_else(|| humanize_path(&path).into(), SharedString::from);
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
            let target = DocTarget::page(self.entries[*ix].path.clone());
            self.workspace
                .update(cx, |workspace, cx| {
                    DocumentationView::open_documentation_page(workspace, Some(target), window, cx);
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

/// Removes a leading YAML front matter block, which the documentation uses for
/// mdBook navigation and search metadata. The block is only stripped when the
/// file opens with a `---` line that is later closed by another `---` line, so
/// a document that legitimately starts with a horizontal rule is preserved.
fn strip_front_matter(content: &str) -> String {
    let Some(rest) = content.strip_prefix("---\n") else {
        return content.to_string();
    };
    let Some(closing) = rest.find("\n---\n") else {
        return content.to_string();
    };
    rest[closing + "\n---\n".len()..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_front_matter_removes_leading_yaml_block() {
        let content = "---\ntitle: AI\n---\n\n# AI\n\nBody\n";

        assert_eq!(strip_front_matter(content), "\n# AI\n\nBody\n");
    }

    #[test]
    fn strip_front_matter_keeps_documents_without_front_matter() {
        let content = "# Getting Started\n\nNo metadata here.\n";

        assert_eq!(strip_front_matter(content), content);
    }

    #[test]
    fn strip_front_matter_keeps_unterminated_block() {
        let content = "---\ntitle: Broken\n\n# Heading\n";

        assert_eq!(strip_front_matter(content), content);
    }

    #[test]
    fn resolve_relative_path_resolves_sibling_and_parent_links() {
        assert_eq!(
            resolve_relative_path("ai/overview.md", "./external-agents.md").as_deref(),
            Some("ai/external-agents.md")
        );
        assert_eq!(
            resolve_relative_path("ai/overview.md", "../getting-started.md").as_deref(),
            Some("getting-started.md")
        );
        assert_eq!(
            resolve_relative_path("development/debuggers.md", "../debugger.md").as_deref(),
            Some("debugger.md")
        );
    }

    #[test]
    fn resolve_relative_path_adds_markdown_extension() {
        assert_eq!(
            resolve_relative_path("index.md", "getting-started").as_deref(),
            Some("getting-started.md")
        );
    }

    #[test]
    fn resolve_relative_path_refuses_to_escape_the_documentation_root() {
        assert_eq!(resolve_relative_path("index.md", "../secrets.md"), None);
        assert_eq!(
            resolve_relative_path("ai/overview.md", "../../outside.md"),
            None
        );
    }

    #[test]
    fn resolve_doc_link_carries_the_fragment() {
        let link = resolve_doc_link("ai/overview.md", "../getting-started.md#command-palette");
        let Some(DocLink::Docs(target)) = link else {
            panic!("expected a documentation link");
        };
        assert_eq!(target.path.as_ref(), "getting-started.md");
        assert_eq!(target.fragment.as_deref(), Some("command-palette"));
    }

    #[test]
    fn resolve_doc_link_treats_bare_fragment_as_current_page() {
        let link = resolve_doc_link("repl.md", "#python");
        let Some(DocLink::Docs(target)) = link else {
            panic!("expected a documentation link");
        };
        assert_eq!(target.path.as_ref(), "repl.md");
        assert_eq!(target.fragment.as_deref(), Some("python"));
    }

    #[test]
    fn resolve_doc_link_classifies_external_and_action_links() {
        assert!(matches!(
            resolve_doc_link("index.md", "https://example.com"),
            Some(DocLink::External(_))
        ));
        assert!(matches!(
            resolve_doc_link("index.md", "zzz://kb/editor::MoveUp"),
            Some(DocLink::Keybinding(_))
        ));
        assert!(matches!(
            resolve_doc_link("index.md", "zzz://docs/key-bindings"),
            Some(DocLink::Docs(target)) if target.path.as_ref() == "key-bindings"
        ));
    }

    #[test]
    fn resolve_doc_link_ignores_links_that_escape_the_root() {
        assert!(resolve_doc_link("index.md", "../../etc/passwd").is_none());
    }
}
