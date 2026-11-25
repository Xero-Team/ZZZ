use editor::{Editor, EditorEvent};
use gpui::{
    Action, AppContext, Entity, EventEmitter, FocusHandle, Focusable, ListAlignment, Point,
    Subscription, Task, Window, actions,
};
use i18n::tr;
use schemars::JsonSchema;
use serde::Deserialize;
use std::{
    collections::HashMap,
    ops::RangeInclusive,
    time::{Duration, Instant},
};

use crate::table_data_engine::{DisplayToDataMapping, TableDataEngine};
use ui::{
    AbsoluteLength, ResizableColumnsState, SharedString, TableInteractionState,
    TableResizeBehavior, prelude::*,
};
use workspace::{Item, SplitDirection, Workspace};

use crate::{
    parser::EditorState,
    settings::CsvPreviewSettings,
    types::{AnyColumn, DataCellId, DisplayRow, TableLikeContent},
};

mod parser;
mod renderer;
mod settings;
mod table_data_engine;
mod types;

actions!(csv, [OpenPreview, OpenPreviewToTheSide, ClearSelection]);

/// Which axis direction a keyboard navigation action moves along.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDirection {
    #[default]
    Up,
    Down,
    Left,
    Right,
}

/// How far a keyboard navigation action moves.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoveUnit {
    /// Move one cell. Default when `unit` is omitted in JSON keybindings.
    #[default]
    Cell,
    /// Move by one visible page (vertical only; horizontal degrades to `Cell`).
    Page,
    /// Jump to the first or last cell on the axis.
    Edge,
}

/// Moves the keyboard cursor one cell, page, or edge in `direction`.
#[derive(Clone, Deserialize, PartialEq, JsonSchema, Action)]
#[action(namespace = csv)]
#[serde(deny_unknown_fields)]
pub struct MoveFocusedCell {
    pub direction: NavigationDirection,
    #[serde(default)]
    pub unit: MoveUnit,
}

/// Extends the cell selection in `direction`, keeping the anchor fixed.
#[derive(Clone, Deserialize, PartialEq, JsonSchema, Action)]
#[action(namespace = csv)]
#[serde(deny_unknown_fields)]
pub struct ExtendSelection {
    pub direction: NavigationDirection,
    #[serde(default)]
    pub unit: MoveUnit,
}

/// The keyboard cursor: which cell is currently focused for navigation.
/// Both fields are stored in data-space so the selection survives sort/filter changes.
/// In single-cell navigation, `anchor == focus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSelection {
    pub anchor: DataCellId,
    pub focus: DataCellId,
}

impl CellSelection {
    pub fn new(anchor: DataCellId, focus: DataCellId) -> Self {
        Self { anchor, focus }
    }

    pub fn single_cell(cell: DataCellId) -> Self {
        Self {
            anchor: cell,
            focus: cell,
        }
    }

    pub fn is_single_cell(&self) -> bool {
        self.anchor == self.focus
    }

    /// Display-space bounding box `(rows, columns)` of the selection, or `None`
    /// when either end is hidden by a filter.
    pub fn display_bounds(
        &self,
        d2d: &DisplayToDataMapping,
    ) -> Option<(RangeInclusive<usize>, RangeInclusive<usize>)> {
        let anchor_row = d2d.get_display_row(self.anchor.row)?.0;
        let focus_row = d2d.get_display_row(self.focus.row)?.0;
        let min_row = anchor_row.min(focus_row);
        let max_row = anchor_row.max(focus_row);
        let min_col = self.anchor.col.0.min(self.focus.col.0);
        let max_col = self.anchor.col.0.max(self.focus.col.0);
        Some((min_row..=max_row, min_col..=max_col))
    }
}

fn valid_custom_delimiter(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let delimiter = characters.next()?;
    (characters.next().is_none() && delimiter != '"' && delimiter != '\r' && delimiter != '\n')
        .then_some(delimiter)
}

pub struct CsvPreviewView {
    pub(crate) engine: TableDataEngine,

    pub(crate) focus_handle: FocusHandle,
    active_editor_state: EditorState,
    custom_delimiter_editor: Entity<Editor>,
    _custom_delimiter_subscription: Subscription,
    pub(crate) custom_delimiter_error: bool,
    pub(crate) table_interaction_state: Entity<TableInteractionState>,
    pub(crate) column_widths: ColumnWidths,
    pub(crate) parsing_task: Option<Task<anyhow::Result<()>>>,
    pub(crate) settings: CsvPreviewSettings,
    pub(crate) detected_delimiter: Option<char>,
    /// Performance metrics for debugging and monitoring CSV operations.
    pub(crate) performance_metrics: PerformanceMetrics,
    pub(crate) list_state: gpui::ListState,
    /// Background task computing the display-to-data mapping after a filter or
    /// sort change. Stored so a new change cancels the previous computation.
    pub(crate) filter_sort_task: Option<Task<()>>,
    /// The keyboard navigation cursor. `None` until the user first navigates.
    pub(crate) selection: Option<CellSelection>,
    /// Cached row height, refreshed from the real text line height on every
    /// render so list sizing tracks font changes instead of a hardcoded guess.
    pub(crate) row_height: Pixels,
    /// Time when the last parsing operation ended, used for smart debouncing
    pub(crate) last_parse_end_time: Option<std::time::Instant>,
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        CsvPreviewView::register(workspace);
    })
    .detach()
}

impl CsvPreviewView {
    pub(crate) fn sync_column_widths(&self, cx: &mut Context<Self>) {
        // plus 1 for the row identifier column
        let cols = self.engine.contents.headers.cols() + 1;
        let line_number_width = self.calculate_row_identifier_column_width();

        let widths = estimate_column_widths(&self.engine.contents, line_number_width);

        let mut resize_behaviors = vec![TableResizeBehavior::Resizable; cols];
        resize_behaviors[0] = TableResizeBehavior::None;

        self.column_widths.widths.update(cx, |state, _cx| {
            if state.cols() == cols {
                for (column_index, (width, resize_behavior)) in
                    widths.into_iter().zip(resize_behaviors).enumerate()
                {
                    if column_index == 0 {
                        state.set_column_configuration(column_index, width, resize_behavior);
                    } else {
                        state.sync_auto_column_configuration(column_index, width, resize_behavior);
                    }
                }
            } else {
                *state = ResizableColumnsState::new(cols, widths, resize_behaviors);
            }
        });
    }

    pub fn register(workspace: &mut Workspace) {
        workspace.register_action_renderer(|div, _, _, cx| {
            div.on_action(cx.listener(|workspace, _: &OpenPreview, window, cx| {
                if let Some(editor) = workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                    .filter(|editor| Self::is_delimited_file(editor, cx))
                {
                    let csv_preview = Self::new(&editor, window, cx);
                    workspace.active_pane().update(cx, |pane, cx| {
                        let existing = pane
                            .items_of_type::<CsvPreviewView>()
                            .find(|view| view.read(cx).active_editor_state.editor == editor);
                        if let Some(idx) = existing.and_then(|e| pane.index_for_item(&e)) {
                            pane.activate_item(idx, true, true, window, cx);
                        } else {
                            pane.add_item(Box::new(csv_preview), true, true, None, window, cx);
                        }
                    });
                    cx.notify();
                }
            }))
            .on_action(cx.listener(
                |workspace, _: &OpenPreviewToTheSide, window, cx| {
                    if let Some(editor) = workspace
                        .active_item(cx)
                        .and_then(|item| item.act_as::<Editor>(cx))
                        .filter(|editor| Self::is_delimited_file(editor, cx))
                    {
                        let csv_preview = Self::new(&editor, window, cx);
                        let pane = workspace
                            .find_pane_in_direction(SplitDirection::Right, cx)
                            .unwrap_or_else(|| {
                                workspace.split_pane(
                                    workspace.active_pane().clone(),
                                    SplitDirection::Right,
                                    window,
                                    cx,
                                )
                            });
                        pane.update(cx, |pane, cx| {
                            let existing = pane
                                .items_of_type::<CsvPreviewView>()
                                .find(|view| view.read(cx).active_editor_state.editor == editor);
                            if let Some(idx) = existing.and_then(|e| pane.index_for_item(&e)) {
                                pane.activate_item(idx, true, true, window, cx);
                            } else {
                                pane.add_item(
                                    Box::new(csv_preview),
                                    false,
                                    false,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                        });
                        cx.notify();
                    }
                },
            ))
        });
    }

    fn new(
        editor: &Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let contents = TableLikeContent::default();
        let custom_delimiter_placeholder = tr(
            cx,
            "csv_preview.settings.custom_delimiter_placeholder",
            "Custom delimiter",
        );
        let custom_delimiter_editor = cx.new(|cx| Editor::single_line(window, cx));
        custom_delimiter_editor.update(cx, |editor, cx| {
            editor.set_placeholder_text(&custom_delimiter_placeholder, window, cx);
        });
        let table_interaction_state = cx.new(|cx| {
            TableInteractionState::new(cx).with_custom_scrollbar(ui::Scrollbars::for_settings::<
                editor::EditorSettingsScrollbarProxy,
            >())
        });
        let row_height = window.pixel_snap(window.line_height());

        cx.new(|cx| {
            let subscription = cx.subscribe(
                editor,
                |this: &mut CsvPreviewView, _editor, event: &EditorEvent, cx| match event {
                    EditorEvent::Edited { .. } | EditorEvent::DirtyChanged => {
                        this.parse_delimited_from_active_editor(true, cx);
                    }
                    _ => {}
                },
            );
            let custom_delimiter_subscription = cx.subscribe(
                &custom_delimiter_editor,
                |this: &mut CsvPreviewView, editor, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::Edited { .. }) {
                        let text = editor.read(cx).text(cx);
                        if let Some(delimiter) = valid_custom_delimiter(&text) {
                            this.settings.delimiter =
                                crate::settings::DelimiterSelection::Character(delimiter);
                            this.custom_delimiter_error = false;
                            this.parse_delimited_from_active_editor(false, cx);
                        } else {
                            this.custom_delimiter_error = !text.is_empty();
                        }
                        cx.notify();
                    }
                },
            );

            let mut view = CsvPreviewView {
                focus_handle: cx.focus_handle(),
                active_editor_state: EditorState {
                    editor: editor.clone(),
                    _subscription: subscription,
                },
                custom_delimiter_editor,
                _custom_delimiter_subscription: custom_delimiter_subscription,
                custom_delimiter_error: false,
                table_interaction_state,
                column_widths: ColumnWidths::new(cx, 1),
                parsing_task: None,
                performance_metrics: PerformanceMetrics::default(),
                list_state: gpui::ListState::new(contents.rows.len(), ListAlignment::Top, px(1.))
                    .measure_all(),
                filter_sort_task: None,
                selection: None,
                row_height,
                settings: CsvPreviewSettings::default(),
                detected_delimiter: None,
                last_parse_end_time: None,
                engine: TableDataEngine::default(),
            };

            view.parse_delimited_from_active_editor(false, cx);
            view
        })
    }

    pub(crate) fn editor_state(&self) -> &EditorState {
        &self.active_editor_state
    }

    pub(crate) fn clear_filters(&mut self, col: AnyColumn, cx: &mut Context<Self>) {
        self.engine.clear_filters_for_col(col);
        self.apply_filter_sort(cx);
    }

    pub(crate) fn toggle_filter(
        &mut self,
        col: AnyColumn,
        value: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        if let Err(err) = self.engine.toggle_filter(col, value) {
            log::error!("Failed to toggle filter: {err}");
            return;
        }
        self.apply_filter_sort(cx);
    }

    /// Recomputes the display-to-data mapping after a filter or sort change on a
    /// background task, so a large table does not stall the foreground. Storing
    /// the task cancels any previous in-flight computation.
    pub(crate) fn apply_filter_sort(&mut self, cx: &mut Context<Self>) {
        let contents = self.engine.contents.clone();
        let filter_stack = self.engine.filter_stack.clone();
        let sorting = self.engine.applied_sorting;

        self.filter_sort_task = Some(cx.spawn(async move |this, cx| {
            let start = Instant::now();
            let mapping = cx
                .background_spawn(async move {
                    DisplayToDataMapping::compute(&contents, &filter_stack, sorting)
                })
                .await;
            let elapsed = start.elapsed();

            this.update(cx, |view, cx| {
                view.performance_metrics
                    .insert_timing("Filter&sort", elapsed);
                view.engine.set_d2d_mapping(mapping);
                let visible_rows = view.engine.d2d_mapping().visible_row_count();
                view.list_state.reset(visible_rows);
                cx.notify();
            })
            .ok();
        }));
    }

    pub(crate) fn move_focused_cell(
        &mut self,
        action: &MoveFocusedCell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(action.direction, action.unit, false, window, cx);
    }

    pub(crate) fn extend_selection(
        &mut self,
        action: &ExtendSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(action.direction, action.unit, true, window, cx);
    }

    pub(crate) fn clear_selection(
        &mut self,
        _: &ClearSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.take().is_some() {
            cx.notify();
        }
    }

    fn navigate(
        &mut self,
        direction: NavigationDirection,
        unit: MoveUnit,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row_count = self.engine.d2d_mapping().visible_row_count();
        let column_count = self.engine.contents.number_of_cols;

        if row_count == 0 || column_count == 0 {
            return;
        }

        let (anchor, current_position) = if let Some(selection) = self.selection.as_ref() {
            let position = self
                .engine
                .d2d_mapping()
                .get_display_row(selection.focus.row)
                .map_or((0, 0), |row| (row.0, selection.focus.col.0));
            (selection.anchor, position)
        } else {
            let Some(initial_row) = self.engine.d2d_mapping().get_data_row(DisplayRow(0)) else {
                return;
            };
            (DataCellId::new(initial_row, AnyColumn(0)), (0, 0))
        };

        let (new_row, new_column) = if self.selection.is_none() && !extend && unit == MoveUnit::Cell
        {
            // The first single-cell move initializes focus at (0, 0) without skipping.
            (0, 0)
        } else {
            self.compute_move(current_position, direction, unit)
        };

        let Some(new_data_row) = self.engine.d2d_mapping().get_data_row(DisplayRow(new_row)) else {
            return;
        };
        let new_cell = DataCellId::new(new_data_row, AnyColumn(new_column));

        self.selection = Some(if extend {
            CellSelection::new(anchor, new_cell)
        } else {
            CellSelection::single_cell(new_cell)
        });

        self.scroll_to_reveal_row(new_row, direction);
        self.scroll_to_reveal_column(new_column, window, cx);
        cx.notify();
    }

    /// Where the cursor lands after a move from `current`, clamped to the table.
    fn compute_move(
        &self,
        current: (usize, usize),
        direction: NavigationDirection,
        unit: MoveUnit,
    ) -> (usize, usize) {
        let row_count = self.engine.d2d_mapping().visible_row_count();
        let column_count = self.engine.contents.number_of_cols;
        let (row, column) = current;

        let item_height = self.row_height + px(1.0);
        let viewport_height = self.list_state.viewport_bounds().size.height;
        let visible_rows = if item_height > px(0.) && viewport_height > px(0.) {
            (viewport_height / item_height).floor() as usize
        } else {
            1
        };
        // One row of context overlap: moving by (visible_rows - 2) leaves exactly
        // one row of context, matching the one-row reveal cushion below.
        let page_rows = visible_rows.saturating_sub(2).max(1);

        match direction {
            NavigationDirection::Up => {
                let delta = match unit {
                    MoveUnit::Cell => 1,
                    MoveUnit::Page => page_rows,
                    MoveUnit::Edge => row,
                };
                (row.saturating_sub(delta), column)
            }
            NavigationDirection::Down => {
                let delta = match unit {
                    MoveUnit::Cell => 1,
                    MoveUnit::Page => page_rows,
                    MoveUnit::Edge => row_count.saturating_sub(1).saturating_sub(row),
                };
                ((row + delta).min(row_count.saturating_sub(1)), column)
            }
            NavigationDirection::Left => {
                let delta = match unit {
                    MoveUnit::Cell | MoveUnit::Page => 1,
                    MoveUnit::Edge => column,
                };
                (row, column.saturating_sub(delta))
            }
            NavigationDirection::Right => {
                let delta = match unit {
                    MoveUnit::Cell | MoveUnit::Page => 1,
                    MoveUnit::Edge => column_count.saturating_sub(1).saturating_sub(column),
                };
                (row, (column + delta).min(column_count.saturating_sub(1)))
            }
        }
    }

    fn scroll_to_reveal_row(&self, row: usize, direction: NavigationDirection) {
        let row_count = self.engine.d2d_mapping().visible_row_count();
        if row_count == 0 {
            return;
        }

        match direction {
            NavigationDirection::Down => {
                // Reveal row + 1 so the focused cell isn't hidden behind the
                // bottom border or the horizontal scrollbar.
                let reveal_row = (row + 1).min(row_count.saturating_sub(1));
                self.list_state.scroll_to_reveal_item(reveal_row);
            }
            NavigationDirection::Up => {
                self.list_state.scroll_to_reveal_item(row.saturating_sub(1));
            }
            NavigationDirection::Left | NavigationDirection::Right => {
                self.list_state.scroll_to_reveal_item(row);
            }
        }
    }

    fn scroll_to_reveal_column(&self, column_index: usize, window: &Window, cx: &App) {
        let handle = self
            .table_interaction_state
            .read(cx)
            .horizontal_scroll_handle
            .clone();

        let widths_state = self.column_widths.widths.read(cx);
        let total_cols = widths_state.cols();
        if total_cols <= 1 || column_index + 2 > total_cols {
            return;
        }

        let rem_size = window.rem_size();
        let peek_padding = px(20.0);

        let left_px = widths_state.pinned_width(column_index + 1, rem_size)
            - widths_state.pinned_width(1, rem_size);
        let right_px = widths_state.pinned_width(column_index + 2, rem_size)
            - widths_state.pinned_width(1, rem_size);

        let scroll_x = -handle.offset().x;
        let viewport_width = handle.bounds().size.width;

        if left_px < scroll_x {
            handle.set_offset(Point::new(-left_px, px(0.)));
        } else if right_px + peek_padding > scroll_x + viewport_width {
            let target = (right_px + peek_padding) - viewport_width;
            handle.set_offset(Point::new(-target, px(0.)));
        }
    }

    pub fn resolve_active_item_as_delimited_editor(
        workspace: &Workspace,
        cx: &mut Context<Workspace>,
    ) -> Option<Entity<Editor>> {
        let editor = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))?;
        Self::is_delimited_file(&editor, cx).then_some(editor)
    }

    fn is_delimited_file(editor: &Entity<Editor>, cx: &App) -> bool {
        editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .is_some_and(|buffer| {
                buffer
                    .read(cx)
                    .file()
                    .and_then(|file| file.path().extension())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("csv")
                            || extension.eq_ignore_ascii_case("tsv")
                            || extension.eq_ignore_ascii_case("psv")
                            || extension.eq_ignore_ascii_case("scsv")
                            || extension.eq_ignore_ascii_case("ssv")
                    })
            })
    }
}

impl Focusable for CsvPreviewView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for CsvPreviewView {}

impl Item for CsvPreviewView {
    type Event = ();

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::FileDoc))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.editor_state()
            .editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|b| {
                let file = b.read(cx).file()?;
                file.path().file_name().map(|name| {
                    tr(cx, "csv_preview.preview_title", "Preview {}")
                        .replacen("{}", name, 1)
                        .into()
                })
            })
            .unwrap_or_else(|| tr(cx, "csv_preview.tab_title", "Delimited Text Preview").into())
    }
}

#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    /// Map of timing metrics with their duration and measurement time.
    pub timings: HashMap<&'static str, (Duration, Instant)>,
    /// List of display indices that were rendered in the current frame.
    pub rendered_indices: Vec<usize>,
}
impl PerformanceMetrics {
    /// Records a timing metric measured elsewhere, e.g. on a background task.
    pub fn insert_timing(&mut self, name: &'static str, duration: Duration) {
        self.timings.insert(name, (duration, Instant::now()));
    }

    /// Displays all metrics sorted A-Z in format: `{name}: {took}ms {ago}s ago`
    pub fn display(&self) -> String {
        let mut metrics = self.timings.iter().collect::<Vec<_>>();
        metrics.sort_by_key(|&(name, _)| *name);
        metrics
            .iter()
            .map(|(name, (duration, time))| {
                let took = duration.as_secs_f32() * 1000.;
                let ago = time.elapsed().as_secs();
                format!("{name}: {took:.2}ms {ago}s ago")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get timing for a specific metric
    pub fn get_timing(&self, name: &str) -> Option<Duration> {
        self.timings.get(name).map(|(duration, _)| *duration)
    }
}

/// Holds state of column widths for a table component in CSV preview.
pub(crate) struct ColumnWidths {
    pub widths: Entity<ResizableColumnsState>,
}

impl ColumnWidths {
    pub(crate) fn new(cx: &mut Context<CsvPreviewView>, cols: usize) -> Self {
        Self {
            widths: cx.new(|_cx| {
                ResizableColumnsState::new(
                    cols,
                    vec![AbsoluteLength::Pixels(px(150.)); cols],
                    vec![ui::TableResizeBehavior::Resizable; cols],
                )
            }),
        }
    }
}

const DEFAULT_COLUMN_WIDTH_PX: f32 = 150.0;
const MIN_DATA_COLUMN_WIDTH_PX: f32 = 96.0;
const MAX_DATA_COLUMN_WIDTH_PX: f32 = 420.0;
const ESTIMATED_CHAR_WIDTH_PX: f32 = 8.0;
const CELL_HORIZONTAL_PADDING_PX: f32 = 24.0;
const WIDTH_ESTIMATION_SAMPLE_ROWS: usize = 200;

fn estimate_column_widths(
    contents: &TableLikeContent,
    row_identifier_width: f32,
) -> Vec<AbsoluteLength> {
    let mut widths = Vec::with_capacity(contents.number_of_cols + 1);
    widths.push(AbsoluteLength::Pixels(px(row_identifier_width)));

    for column_index in 0..contents.number_of_cols {
        let content_width = estimate_data_column_width_px(contents, column_index);
        widths.push(AbsoluteLength::Pixels(px(content_width)));
    }

    widths
}

fn estimate_data_column_width_px(contents: &TableLikeContent, column_index: usize) -> f32 {
    let header_width = contents
        .headers
        .get(column_index)
        .map_or(DEFAULT_COLUMN_WIDTH_PX, estimated_cell_width_px);

    let sample_width = contents
        .rows
        .iter()
        .take(WIDTH_ESTIMATION_SAMPLE_ROWS)
        .filter_map(|row| row.get(column_index))
        .map(estimated_cell_width_px)
        .fold(header_width, f32::max);

    sample_width.clamp(MIN_DATA_COLUMN_WIDTH_PX, MAX_DATA_COLUMN_WIDTH_PX)
}

fn estimated_cell_width_px(cell: &crate::types::TableCell) -> f32 {
    let Some(value) = cell.display_value() else {
        return MIN_DATA_COLUMN_WIDTH_PX;
    };

    let longest_line_chars = value
        .as_ref()
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    if longest_line_chars == 0 {
        MIN_DATA_COLUMN_WIDTH_PX
    } else {
        (longest_line_chars as f32) * ESTIMATED_CHAR_WIDTH_PX + CELL_HORIZONTAL_PADDING_PX
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn csv_preview_defaults_enable_multiline_variable_height() {
        let settings = CsvPreviewSettings::default();

        assert!(matches!(
            settings.rendering_with,
            crate::settings::RowRenderMechanism::VariableList
        ));
        assert!(settings.multiline_cells_enabled);
    }

    #[test]
    fn csv_preview_estimates_column_widths_from_multiline_and_long_values() {
        let contents = TableLikeContent::from_str(
            "name,description,notes\nalpha,short,ok\nbeta,very very long content,wrapped value\ngamma,first line\nsecond line,end"
                .to_string(),
        );

        let widths = estimate_column_widths(&contents, 60.0);

        assert_eq!(widths.len(), 4);
        assert_eq!(widths[0], AbsoluteLength::Pixels(px(60.0)));
        assert_matches!(widths[1], AbsoluteLength::Pixels(width) if width >= px(MIN_DATA_COLUMN_WIDTH_PX)
        );
        assert_matches!(widths[2], AbsoluteLength::Pixels(width) if width > px(150.0));
        assert_matches!(widths[3], AbsoluteLength::Pixels(width) if width >= px(MIN_DATA_COLUMN_WIDTH_PX)
        );
    }
}
