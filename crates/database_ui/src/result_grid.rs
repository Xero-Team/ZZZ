//! Display state for the query result grid: column widths and sort order.
//!
//! Sorting is client-side over the loaded window because the row budget already
//! bounds what the view holds. Re-running the query with a generated `ORDER BY`
//! would change semantics the user cannot see, so the view says when a sort only
//! covers the loaded rows.

use database_core::{ResultSet, Value};
use gpui::{AbsoluteLength, App, AppContext as _, Entity, px};
use ui::{ResizableColumnsState, TableInteractionState, TableResizeBehavior};

const MIN_COLUMN_WIDTH_PX: f32 = 96.0;
const MAX_COLUMN_WIDTH_PX: f32 = 420.0;
const ESTIMATED_CHAR_WIDTH_PX: f32 = 8.0;
const CELL_HORIZONTAL_PADDING_PX: f32 = 24.0;
const WIDTH_ESTIMATION_SAMPLE_ROWS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// How a cell should be presented. Kept separate from rendering so the typing
/// rules (NULL vs number vs binary vs text) are unit tested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellClass {
    Null,
    Number,
    Binary,
    Text,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellPresentation {
    /// Text shown in the grid.
    pub text: String,
    /// Text copied or exported. NULL copies as an empty field.
    pub copy_text: String,
    pub class: CellClass,
}

impl CellPresentation {
    pub fn for_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::null();
        };
        let class = match value {
            Value::Null => CellClass::Null,
            Value::Integer(_) | Value::Real(_) | Value::Numeric(_) => CellClass::Number,
            Value::Blob(_) => CellClass::Binary,
            _ => CellClass::Text,
        };
        Self {
            text: value.display(),
            copy_text: value.to_export_text(),
            class,
        }
    }

    fn null() -> Self {
        Self {
            text: "NULL".to_owned(),
            copy_text: String::new(),
            class: CellClass::Null,
        }
    }
}

/// Resizable column state plus the display order produced by sorting.
pub struct ResultGridState {
    pub column_widths: Entity<ResizableColumnsState>,
    pub interaction: Entity<TableInteractionState>,
    sort: Option<(usize, SortDirection)>,
    order: Vec<usize>,
    columns: usize,
}

impl ResultGridState {
    pub fn new(cx: &mut App) -> Self {
        let column_widths = cx.new(|_| {
            ResizableColumnsState::new(
                1,
                vec![AbsoluteLength::Pixels(px(MIN_COLUMN_WIDTH_PX))],
                vec![TableResizeBehavior::Resizable],
            )
        });
        Self {
            column_widths,
            interaction: cx.new(|cx| TableInteractionState::new(cx)),
            sort: None,
            order: Vec::new(),
            columns: 0,
        }
    }

    /// The active sort column and direction.
    pub fn sort(&self) -> Option<(usize, SortDirection)> {
        self.sort
    }

    /// Row indices in display order.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Adopt a new result set. Column widths are preserved while the column count
    /// stays the same so a re-run does not discard the user's resizing. Passing
    /// `None` only clears the display order (the tab has not run a query yet).
    pub fn sync(&mut self, result: Option<&ResultSet>, cx: &mut App) {
        let Some(result) = result else {
            self.order.clear();
            return;
        };
        let columns = result.columns.len();
        if columns > 0 && columns != self.columns {
            let widths = Vec::from_fn(columns, |index| {
                AbsoluteLength::Pixels(px(estimate_column_width(result, index)))
            });
            self.column_widths = cx.new(|_| {
                ResizableColumnsState::new(
                    columns,
                    widths,
                    vec![TableResizeBehavior::Resizable; columns],
                )
            });
        }
        self.columns = columns;
        self.order = compute_order(Some(result), self.sort);
    }

    /// Cycle one column's sort state: ascending, descending, then off.
    pub fn toggle_sort(&mut self, column: usize, result: Option<&ResultSet>) {
        self.sort = next_sort(self.sort, column);
        self.order = compute_order(result, self.sort);
    }
}

/// The sort state after clicking `column`.
pub fn next_sort(
    current: Option<(usize, SortDirection)>,
    column: usize,
) -> Option<(usize, SortDirection)> {
    match current {
        Some((active, SortDirection::Ascending)) if active == column => {
            Some((column, SortDirection::Descending))
        }
        Some((active, SortDirection::Descending)) if active == column => None,
        _ => Some((column, SortDirection::Ascending)),
    }
}

/// Display order for the loaded rows. Values keep their type through
/// `Value::sort_key`, so numbers sort numerically rather than as text.
pub fn compute_order(
    result: Option<&ResultSet>,
    sort: Option<(usize, SortDirection)>,
) -> Vec<usize> {
    let Some(result) = result else {
        return Vec::new();
    };
    let mut order = (0..result.rows.len()).collect::<Vec<_>>();
    let Some((column, direction)) = sort else {
        return order;
    };
    if column >= result.columns.len() {
        return order;
    }
    order.sort_by(|left, right| {
        let left_key = result.value_at(*left, column).map(Value::sort_key);
        let right_key = result.value_at(*right, column).map(Value::sort_key);
        let ordering = left_key.cmp(&right_key);
        let ordering = match direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        };
        // Equal values keep their original order in both directions.
        ordering.then_with(|| left.cmp(right))
    });
    order
}

fn estimate_column_width(result: &ResultSet, column: usize) -> f32 {
    let header_chars = result
        .columns
        .get(column)
        .map_or(0, |column| column.name.chars().count());
    let sample_chars = result
        .rows
        .iter()
        .take(WIDTH_ESTIMATION_SAMPLE_ROWS)
        .filter_map(|row| row.get(column))
        .map(|value| {
            value
                .display()
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
        })
        .fold(header_chars, usize::max);
    ((sample_chars as f32) * ESTIMATED_CHAR_WIDTH_PX + CELL_HORIZONTAL_PADDING_PX)
        .clamp(MIN_COLUMN_WIDTH_PX, MAX_COLUMN_WIDTH_PX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database_core::{Column, ColumnType, Row};

    fn text_set(values: &[&str]) -> ResultSet {
        let mut set = ResultSet::new(
            vec![Column::new("value", ColumnType::Text)],
            "SELECT value FROM t",
        );
        for value in values {
            set.push_row(Row(vec![Value::Text((*value).to_owned())]), 100);
        }
        set
    }

    fn number_set(values: &[i64]) -> ResultSet {
        let mut set = ResultSet::new(
            vec![Column::new("value", ColumnType::Integer)],
            "SELECT value FROM t",
        );
        for value in values {
            set.push_row(Row(vec![Value::Integer(*value)]), 100);
        }
        set
    }

    #[test]
    fn numbers_sort_numerically_not_lexically() {
        let set = number_set(&[10, 2, 33]);
        let order = compute_order(Some(&set), Some((0, SortDirection::Ascending)));
        assert_eq!(order, vec![1, 0, 2]);
    }

    #[test]
    fn descending_reverses_values_but_not_ties() {
        let set = text_set(&["b", "a", "b"]);
        let order = compute_order(Some(&set), Some((0, SortDirection::Descending)));
        assert_eq!(order, vec![0, 2, 1]);
    }

    #[test]
    fn nulls_sort_first_ascending_and_last_descending() {
        let mut set = number_set(&[1, 2]);
        set.rows[0] = Row(vec![Value::Null]);
        let ascending = compute_order(Some(&set), Some((0, SortDirection::Ascending)));
        assert_eq!(ascending, vec![0, 1]);
        let descending = compute_order(Some(&set), Some((0, SortDirection::Descending)));
        assert_eq!(descending, vec![1, 0]);
    }

    #[test]
    fn absent_result_has_no_rows() {
        assert!(compute_order(None, Some((0, SortDirection::Ascending))).is_empty());
    }

    #[test]
    fn sort_state_cycles_through_both_directions() {
        let ascending = next_sort(None, 1);
        assert_eq!(ascending, Some((1, SortDirection::Ascending)));
        let descending = next_sort(ascending, 1);
        assert_eq!(descending, Some((1, SortDirection::Descending)));
        assert_eq!(next_sort(descending, 1), None);
        // Clicking another column starts over at ascending.
        assert_eq!(
            next_sort(Some((1, SortDirection::Descending)), 0),
            Some((0, SortDirection::Ascending))
        );
    }

    #[test]
    fn column_widths_grow_with_content_and_stay_within_bounds() {
        let short = text_set(&["a"]);
        let long = text_set(&["a very long value that exceeds the maximum column width"]);
        assert!(estimate_column_width(&short, 0) < estimate_column_width(&long, 0));
        assert_eq!(estimate_column_width(&long, 0), MAX_COLUMN_WIDTH_PX);
        assert!(estimate_column_width(&short, 0) >= MIN_COLUMN_WIDTH_PX);
    }

    #[test]
    fn numeric_values_are_classified_for_right_alignment() {
        let integer = CellPresentation::for_value(Some(&Value::Integer(7)));
        assert_eq!(integer.class, CellClass::Number);
        assert_eq!(integer.text, "7");
        let real = CellPresentation::for_value(Some(&Value::Real(1.5)));
        assert_eq!(real.class, CellClass::Number);
    }

    #[test]
    fn null_cells_display_null_but_copy_as_empty() {
        let null = CellPresentation::for_value(Some(&Value::Null));
        assert_eq!(null.class, CellClass::Null);
        assert_eq!(null.text, "NULL");
        assert_eq!(null.copy_text, "");
        assert_eq!(CellPresentation::for_value(None).class, CellClass::Null);
    }

    #[test]
    fn blobs_show_a_digest_summary_and_are_not_inlined() {
        let blob = CellPresentation::for_value(Some(&Value::Blob(vec![0xde, 0xad, 0xbe])));
        assert_eq!(blob.class, CellClass::Binary);
        assert_eq!(blob.text, "[blob 3B: deadbe]");
    }

    #[test]
    fn text_and_dates_are_plain_text_cells() {
        assert_eq!(
            CellPresentation::for_value(Some(&Value::Text("hello".into()))).class,
            CellClass::Text
        );
        assert_eq!(
            CellPresentation::for_value(Some(&Value::Date("2026-01-02".into()))).class,
            CellClass::Text
        );
    }
}
