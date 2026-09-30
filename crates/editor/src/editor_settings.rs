use core::num;

use gpui::App;
use language::CursorShape;
use project::project_settings::DiagnosticSeverity;
pub use settings::{
    CodeLens, CompletionDetailAlignment, CompletionMenuItemKind, CurrentLineHighlight, DelayMs,
    DiffViewStyle, DisplayIn, DocumentColorsRenderMode, DoubleClickInMultibuffer,
    GoToDefinitionFallback, GoToDefinitionScrollStrategy, LineNumberScale, MinimapThumb,
    MinimapThumbBorder, MultiCursorModifier, ScrollBeyondLastLine, ScrollbarDiagnostics,
    SeedQuerySetting, ShowMinimap, SnippetSortOrder,
};
use settings::{RegisterSetting, RelativeLineNumbers, Settings};
use ui::scrollbars::ShowScrollbar;

/// Imports from the VSCode settings at
/// https://code.visualstudio.com/docs/reference/default-settings
#[derive(Clone, RegisterSetting)]
pub struct EditorSettings {
    pub cursor_blink: bool,
    pub cursor_shape: Option<CursorShape>,
    pub cursor_animation: CursorAnimationSettings,
    pub smooth_scroll: SmoothScrollSettings,
    pub current_line_highlight: CurrentLineHighlight,
    pub selection_highlight: bool,
    pub rounded_selection: bool,
    pub lsp_highlight_debounce: DelayMs,
    pub hover_popover_enabled: bool,
    pub hover_popover_delay: DelayMs,
    pub hover_popover_sticky: bool,
    pub hover_popover_hiding_delay: DelayMs,
    pub toolbar: Toolbar,
    pub scrollbar: Scrollbar,
    pub minimap: Minimap,
    pub gutter: Gutter,
    pub scroll_beyond_last_line: ScrollBeyondLastLine,
    pub vertical_scroll_margin: f64,
    pub autoscroll_on_clicks: bool,
    pub horizontal_scroll_margin: f32,
    pub scroll_sensitivity: f32,
    pub mouse_wheel_zoom: bool,
    pub fast_scroll_sensitivity: f32,
    pub sticky_scroll: StickyScroll,
    pub relative_line_numbers: RelativeLineNumbers,
    pub seed_search_query_from_cursor: SeedQuerySetting,
    pub use_smartcase_search: bool,
    pub multi_cursor_modifier: MultiCursorModifier,
    pub redact_private_values: bool,
    pub expand_excerpt_lines: u32,
    pub excerpt_context_lines: u32,
    pub middle_click_paste: bool,
    pub double_click_in_multibuffer: DoubleClickInMultibuffer,
    pub search_wrap: bool,
    pub search: SearchSettings,
    pub auto_signature_help: bool,
    pub language_detection: bool,
    pub show_signature_help_after_edits: bool,
    pub go_to_definition_fallback: GoToDefinitionFallback,
    pub go_to_definition_scroll_strategy: GoToDefinitionScrollStrategy,
    pub jupyter: Jupyter,
    pub snippet_sort_order: SnippetSortOrder,
    pub diagnostics_max_severity: Option<DiagnosticSeverity>,
    pub inline_code_actions: bool,
    pub drag_and_drop_selection: DragAndDropSelection,
    pub code_lens: CodeLens,
    pub lsp_document_colors: DocumentColorsRenderMode,
    pub lsp_document_links: bool,
    pub minimum_contrast_for_highlights: f32,
    pub completion_menu_scrollbar: ShowScrollbar,
    pub completion_detail_alignment: CompletionDetailAlignment,
    pub completion_menu_item_kind: CompletionMenuItemKind,
    pub diff_view_style: DiffViewStyle,
    pub minimum_split_diff_width: f32,
    pub line_number_scale: LineNumberScale,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CursorAnimationSettings {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SmoothScrollSettings {
    pub enabled: bool,
    pub duration: settings::DelayMs,
}

#[derive(Debug, Clone)]
pub struct Jupyter {
    /// Whether the Jupyter feature is enabled.
    ///
    /// Default: true
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StickyScroll {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolbar {
    pub breadcrumbs: bool,
    pub quick_actions: bool,
    pub selections_menu: bool,
    pub agent_review: bool,
    pub code_actions: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Scrollbar {
    pub show: ShowScrollbar,
    pub git_diff: bool,
    pub selected_text: bool,
    pub selected_symbol: bool,
    pub search_results: bool,
    pub diagnostics: ScrollbarDiagnostics,
    pub cursors: bool,
    pub axes: ScrollbarAxes,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Minimap {
    pub show: ShowMinimap,
    pub display_in: DisplayIn,
    pub thumb: MinimapThumb,
    pub thumb_border: MinimapThumbBorder,
    pub current_line_highlight: Option<CurrentLineHighlight>,
    pub max_width_columns: num::NonZeroU32,
}

impl Minimap {
    pub fn minimap_enabled(&self) -> bool {
        self.show != ShowMinimap::Never
    }

    #[inline]
    pub fn on_active_editor(&self) -> bool {
        self.display_in == DisplayIn::ActiveEditor
    }

    pub fn with_show_override(self) -> Self {
        Self {
            show: ShowMinimap::Always,
            ..self
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Gutter {
    pub min_line_number_digits: usize,
    pub line_numbers: bool,
    pub runnables: bool,
    pub breakpoints: bool,
    pub bookmarks: bool,
    pub folds: bool,
    pub git_gutter_width: Option<f32>,
}

/// Forcefully enable or disable the scrollbar for each axis
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScrollbarAxes {
    /// When false, forcefully disables the horizontal scrollbar. Otherwise, obey other settings.
    ///
    /// Default: true
    pub horizontal: bool,

    /// When false, forcefully disables the vertical scrollbar. Otherwise, obey other settings.
    ///
    /// Default: true
    pub vertical: bool,
}

/// Whether to allow drag and drop text selection in buffer.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct DragAndDropSelection {
    /// When true, enables drag and drop text selection in buffer.
    ///
    /// Default: true
    pub enabled: bool,

    /// The delay in milliseconds that must elapse before drag and drop is allowed. Otherwise, a new text selection is created.
    ///
    /// Default: 300
    pub delay: DelayMs,
}

/// Default options for buffer and project search items.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct SearchSettings {
    /// Whether to show the project search button in the status bar.
    pub button: bool,
    /// Whether to only match on whole words.
    pub whole_word: bool,
    /// Whether to match case sensitively.
    pub case_sensitive: bool,
    /// Whether to include gitignored files in search results.
    pub include_ignored: bool,
    /// Whether to interpret the search query as a regular expression.
    pub regex: bool,
    /// Whether to center the cursor on each search match when navigating.
    pub center_on_match: bool,
}

impl EditorSettings {
    pub fn jupyter_enabled(cx: &App) -> bool {
        EditorSettings::get_global(cx).jupyter.enabled
    }
}

impl Settings for EditorSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let editor = content.editor.clone();
        let cursor_animation = editor
            .cursor_animation
            .expect("cursor_animation should be present");
        let smooth_scroll = editor
            .smooth_scroll
            .expect("smooth_scroll should be present");
        let scrollbar = editor.scrollbar.expect("scrollbar should be present");
        let minimap = editor.minimap.expect("minimap should be present");
        let gutter = editor.gutter.expect("gutter should be present");
        let axes = scrollbar.axes.expect("axes should be present");
        let toolbar = editor.toolbar.expect("toolbar should be present");
        let search = editor.search.expect("search should be present");
        let drag_and_drop_selection = editor
            .drag_and_drop_selection
            .expect("drag_and_drop_selection should be present");
        let sticky_scroll = editor
            .sticky_scroll
            .expect("sticky_scroll should be present");
        Self {
            cursor_blink: editor.cursor_blink.expect("cursor_blink should be present"),
            cursor_shape: editor.cursor_shape.map(Into::into),
            cursor_animation: CursorAnimationSettings {
                enabled: cursor_animation.enabled.expect("enabled should be present"),
            },
            smooth_scroll: SmoothScrollSettings {
                enabled: smooth_scroll.enabled.expect("enabled should be present"),
                duration: smooth_scroll.duration.expect("duration should be present"),
            },
            current_line_highlight: editor
                .current_line_highlight
                .expect("current_line_highlight should be present"),
            selection_highlight: editor
                .selection_highlight
                .expect("selection_highlight should be present"),
            rounded_selection: editor
                .rounded_selection
                .expect("rounded_selection should be present"),
            lsp_highlight_debounce: editor
                .lsp_highlight_debounce
                .expect("lsp_highlight_debounce should be present"),
            hover_popover_enabled: editor
                .hover_popover_enabled
                .expect("hover_popover_enabled should be present"),
            hover_popover_delay: editor
                .hover_popover_delay
                .expect("hover_popover_delay should be present"),
            hover_popover_sticky: editor
                .hover_popover_sticky
                .expect("hover_popover_sticky should be present"),
            hover_popover_hiding_delay: editor
                .hover_popover_hiding_delay
                .expect("hover_popover_hiding_delay should be present"),
            toolbar: Toolbar {
                breadcrumbs: toolbar.breadcrumbs.expect("breadcrumbs should be present"),
                quick_actions: toolbar
                    .quick_actions
                    .expect("quick_actions should be present"),
                selections_menu: toolbar
                    .selections_menu
                    .expect("selections_menu should be present"),
                agent_review: toolbar
                    .agent_review
                    .expect("agent_review should be present"),
                code_actions: toolbar
                    .code_actions
                    .expect("code_actions should be present"),
            },
            scrollbar: Scrollbar {
                show: scrollbar
                    .show
                    .map(ui_scrollbar_settings_from_raw)
                    .expect("map should be present"),
                git_diff: scrollbar.git_diff.expect("git_diff should be present")
                    && content
                        .git
                        .as_ref()
                        .expect("value should have the expected type")
                        .enabled
                        .expect("enabled should be present")
                        .is_git_diff_enabled(),
                selected_text: scrollbar
                    .selected_text
                    .expect("selected_text should be present"),
                selected_symbol: scrollbar
                    .selected_symbol
                    .expect("selected_symbol should be present"),
                search_results: scrollbar
                    .search_results
                    .expect("search_results should be present"),
                diagnostics: scrollbar
                    .diagnostics
                    .expect("diagnostics should be present"),
                cursors: scrollbar.cursors.expect("cursors should be present"),
                axes: ScrollbarAxes {
                    horizontal: axes.horizontal.expect("horizontal should be present"),
                    vertical: axes.vertical.expect("vertical should be present"),
                },
            },
            minimap: Minimap {
                show: minimap.show.expect("show should be present"),
                display_in: minimap.display_in.expect("display_in should be present"),
                thumb: minimap.thumb.expect("thumb should be present"),
                thumb_border: minimap
                    .thumb_border
                    .expect("thumb_border should be present"),
                current_line_highlight: minimap.current_line_highlight,
                max_width_columns: minimap
                    .max_width_columns
                    .expect("max_width_columns should be present"),
            },
            gutter: Gutter {
                min_line_number_digits: gutter
                    .min_line_number_digits
                    .expect("min_line_number_digits should be present"),
                line_numbers: gutter.line_numbers.expect("line_numbers should be present"),
                runnables: gutter.runnables.expect("runnables should be present"),
                bookmarks: gutter.bookmarks.expect("bookmarks should be present"),
                breakpoints: gutter.breakpoints.expect("breakpoints should be present"),
                folds: gutter.folds.expect("folds should be present"),
                git_gutter_width: gutter.git_gutter_width,
            },
            scroll_beyond_last_line: editor
                .scroll_beyond_last_line
                .expect("scroll_beyond_last_line should be present"),
            vertical_scroll_margin: editor
                .vertical_scroll_margin
                .expect("vertical_scroll_margin should be present")
                as f64,
            autoscroll_on_clicks: editor
                .autoscroll_on_clicks
                .expect("autoscroll_on_clicks should be present"),
            horizontal_scroll_margin: editor
                .horizontal_scroll_margin
                .expect("horizontal_scroll_margin should be present"),
            scroll_sensitivity: editor
                .scroll_sensitivity
                .expect("scroll_sensitivity should be present"),
            mouse_wheel_zoom: editor
                .mouse_wheel_zoom
                .expect("mouse_wheel_zoom should be present"),
            fast_scroll_sensitivity: editor
                .fast_scroll_sensitivity
                .expect("fast_scroll_sensitivity should be present"),
            sticky_scroll: StickyScroll {
                enabled: sticky_scroll.enabled.expect("enabled should be present"),
            },
            relative_line_numbers: editor
                .relative_line_numbers
                .expect("relative_line_numbers should be present"),
            seed_search_query_from_cursor: editor
                .seed_search_query_from_cursor
                .expect("seed_search_query_from_cursor should be present"),
            use_smartcase_search: editor
                .use_smartcase_search
                .expect("use_smartcase_search should be present"),
            multi_cursor_modifier: editor
                .multi_cursor_modifier
                .expect("multi_cursor_modifier should be present"),
            redact_private_values: editor
                .redact_private_values
                .expect("redact_private_values should be present"),
            expand_excerpt_lines: editor
                .expand_excerpt_lines
                .expect("expand_excerpt_lines should be present"),
            excerpt_context_lines: editor
                .excerpt_context_lines
                .expect("excerpt_context_lines should be present"),
            middle_click_paste: editor
                .middle_click_paste
                .expect("middle_click_paste should be present"),
            double_click_in_multibuffer: editor
                .double_click_in_multibuffer
                .expect("double_click_in_multibuffer should be present"),
            search_wrap: editor.search_wrap.expect("search_wrap should be present"),
            search: SearchSettings {
                button: search.button.expect("button should be present"),
                whole_word: search.whole_word.expect("whole_word should be present"),
                case_sensitive: search
                    .case_sensitive
                    .expect("case_sensitive should be present"),
                include_ignored: search
                    .include_ignored
                    .expect("include_ignored should be present"),
                regex: search.regex.expect("regex should be present"),
                center_on_match: search
                    .center_on_match
                    .expect("center_on_match should be present"),
            },
            auto_signature_help: editor
                .auto_signature_help
                .expect("auto_signature_help should be present"),
            language_detection: editor
                .language_detection
                .expect("language_detection should be present"),
            show_signature_help_after_edits: editor
                .show_signature_help_after_edits
                .expect("show_signature_help_after_edits should be present"),
            go_to_definition_fallback: editor
                .go_to_definition_fallback
                .expect("go_to_definition_fallback should be present"),
            go_to_definition_scroll_strategy: editor
                .go_to_definition_scroll_strategy
                .expect("go_to_definition_scroll_strategy should be present"),
            jupyter: Jupyter {
                enabled: editor
                    .jupyter
                    .expect("jupyter should be present")
                    .enabled
                    .expect("enabled should be present"),
            },
            snippet_sort_order: editor
                .snippet_sort_order
                .expect("snippet_sort_order should be present"),
            diagnostics_max_severity: editor.diagnostics_max_severity.map(Into::into),
            inline_code_actions: editor
                .inline_code_actions
                .expect("inline_code_actions should be present"),
            drag_and_drop_selection: DragAndDropSelection {
                enabled: drag_and_drop_selection
                    .enabled
                    .expect("enabled should be present"),
                delay: drag_and_drop_selection
                    .delay
                    .expect("delay should be present"),
            },
            code_lens: editor.code_lens.expect("code_lens should be present"),
            lsp_document_colors: editor
                .lsp_document_colors
                .expect("lsp_document_colors should be present"),
            lsp_document_links: editor
                .lsp_document_links
                .expect("lsp_document_links should be present"),
            minimum_contrast_for_highlights: editor
                .minimum_contrast_for_highlights
                .expect("minimum_contrast_for_highlights should be present")
                .0,
            completion_menu_scrollbar: editor
                .completion_menu_scrollbar
                .map(ui_scrollbar_settings_from_raw)
                .expect("map should be present"),
            completion_detail_alignment: editor
                .completion_detail_alignment
                .expect("completion_detail_alignment should be present"),
            completion_menu_item_kind: editor
                .completion_menu_item_kind
                .expect("completion_menu_item_kind should be present"),
            diff_view_style: editor
                .diff_view_style
                .expect("diff_view_style should be present"),
            minimum_split_diff_width: editor
                .minimum_split_diff_width
                .expect("minimum_split_diff_width should be present"),
            line_number_scale: editor
                .line_number_scale
                .expect("line_number_scale should be present"),
        }
    }
}

#[derive(Default)]
pub struct EditorSettingsScrollbarProxy;

impl ui::scrollbars::ScrollbarVisibility for EditorSettingsScrollbarProxy {
    fn visibility(&self, cx: &App) -> ShowScrollbar {
        EditorSettings::get_global(cx).scrollbar.show
    }
}

pub fn ui_scrollbar_settings_from_raw(
    value: settings::ShowScrollbar,
) -> ui::scrollbars::ShowScrollbar {
    match value {
        settings::ShowScrollbar::Auto => ShowScrollbar::Auto,
        settings::ShowScrollbar::System => ShowScrollbar::System,
        settings::ShowScrollbar::Always => ShowScrollbar::Always,
        settings::ShowScrollbar::Never => ShowScrollbar::Never,
    }
}
