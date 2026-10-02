use crate::{
    CsvPreviewView,
    settings::{DelimiterSelection, HeaderMode},
    types::TableLikeContent,
    types::{LineNumber, TableCell},
};
use editor::Editor;
use gpui::{AppContext, Context, Entity, Subscription, Task};
use std::time::{Duration, Instant};
use text::BufferSnapshot;
use ui::{SharedString, table_row::TableRow};

pub(crate) const REPARSE_DEBOUNCE: Duration = Duration::from_millis(200);

pub(crate) struct EditorState {
    pub editor: Entity<Editor>,
    pub _subscription: Subscription,
}

impl CsvPreviewView {
    pub(crate) fn parse_delimited_from_active_editor(
        &mut self,
        wait_for_debounce: bool,
        cx: &mut Context<Self>,
    ) {
        let editor = self.active_editor_state.editor.clone();
        self.parsing_task = Some(self.parse_delimited_in_background(wait_for_debounce, editor, cx));
    }

    fn parse_delimited_in_background(
        &mut self,
        wait_for_debounce: bool,
        editor: Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        cx.spawn(async move |view, cx| {
            if wait_for_debounce {
                // Smart debouncing: check if cooldown period has already passed
                let now = Instant::now();
                let should_wait = view.update(cx, |view, _| {
                    if let Some(last_end) = view.last_parse_end_time {
                        let cooldown_until = last_end + REPARSE_DEBOUNCE;
                        if now < cooldown_until {
                            Some(cooldown_until - now)
                        } else {
                            None // Cooldown already passed, parse immediately
                        }
                    } else {
                        None // First parse, no debounce
                    }
                })?;

                if let Some(wait_duration) = should_wait {
                    cx.background_executor().timer(wait_duration).await;
                }
            }

            let parse_input = view.update(cx, |view, cx| {
                let buffer_snapshot = editor
                    .read(cx)
                    .buffer()
                    .read(cx)
                    .as_singleton()
                    .map(|buffer| buffer.read(cx).text_snapshot())?;
                let extension =
                    editor
                        .read(cx)
                        .buffer()
                        .read(cx)
                        .as_singleton()
                        .and_then(|buffer| {
                            buffer
                                .read(cx)
                                .file()
                                .and_then(|file| file.path().extension())
                                .map(str::to_ascii_lowercase)
                        });
                let fallback = extension
                    .as_deref()
                    .and_then(default_delimiter_for_extension)
                    .unwrap_or(',');
                let selection = view.settings.delimiter;
                Some((
                    buffer_snapshot,
                    selection,
                    fallback,
                    view.settings.header_mode,
                ))
            })?;

            let Some((buffer_snapshot, selection, fallback, header_mode)) = parse_input else {
                return Ok(());
            };

            let instant = Instant::now();
            let parsed_content = cx
                .background_spawn(async move {
                    let text = buffer_snapshot.text();
                    let delimiter = effective_delimiter(&text, selection, fallback);
                    let content = from_buffer_with_selection(
                        &buffer_snapshot,
                        selection,
                        fallback,
                        header_mode,
                    );
                    (content, delimiter)
                })
                .await;
            let parse_duration = instant.elapsed();
            let parse_end_time: Instant = Instant::now();
            log::debug!("Parsed delimited text in {}ms", parse_duration.as_millis());
            view.update(cx, move |view, cx| {
                view.performance_metrics
                    .timings
                    .insert("Parsing", (parse_duration, Instant::now()));

                let (parsed_content, delimiter) = parsed_content;
                log::debug!("Parsed {} rows", parsed_content.rows.len());
                view.detected_delimiter = Some(delimiter);
                view.engine.contents = parsed_content;
                view.sync_column_widths(cx);
                view.last_parse_end_time = Some(parse_end_time);

                view.apply_filter_sort();
                cx.notify();
            })
        })
    }
}

#[allow(dead_code)]
pub fn from_buffer(buffer_snapshot: &BufferSnapshot, delimiter: char) -> TableLikeContent {
    from_buffer_with_options(buffer_snapshot, delimiter, HeaderMode::FirstRow, false)
}

pub(crate) fn from_buffer_with_selection(
    buffer_snapshot: &BufferSnapshot,
    selection: DelimiterSelection,
    fallback: char,
    header_mode: HeaderMode,
) -> TableLikeContent {
    let text = buffer_snapshot.text();
    let delimiter = effective_delimiter(&text, selection, fallback);
    let skip_directive = has_separator_directive(&text);
    from_buffer_with_options(buffer_snapshot, delimiter, header_mode, skip_directive)
}

fn effective_delimiter(text: &str, selection: DelimiterSelection, fallback: char) -> char {
    if let Some(delimiter) = separator_directive(text) {
        return delimiter;
    }
    match selection {
        DelimiterSelection::Auto if fallback == ',' => detect_delimiter(text, fallback).0,
        DelimiterSelection::Auto => fallback,
        DelimiterSelection::Character(delimiter) => delimiter,
    }
}

fn from_buffer_with_options(
    buffer_snapshot: &BufferSnapshot,
    delimiter: char,
    header_mode: HeaderMode,
    skip_directive: bool,
) -> TableLikeContent {
    let text = buffer_snapshot.text();

    if text.trim().is_empty() {
        return TableLikeContent::default();
    }

    let (mut parsed_cells_with_positions, mut line_numbers) =
        parse_delimited_with_positions(&text, delimiter);
    if skip_directive && !parsed_cells_with_positions.is_empty() {
        parsed_cells_with_positions.remove(0);
        line_numbers.remove(0);
    }
    if parsed_cells_with_positions.is_empty() {
        return TableLikeContent::default();
    }

    // Calculating the longest row, as CSV might have less headers than max row width
    let Some(max_number_of_cols) = parsed_cells_with_positions.iter().map(|r| r.len()).max() else {
        return TableLikeContent::default();
    };

    // Convert to TableCell objects with buffer positions
    let (headers, rows, row_line_numbers) = match header_mode {
        HeaderMode::FirstRow => {
            let raw_headers = parsed_cells_with_positions.remove(0);
            let headers = create_table_row(&buffer_snapshot, max_number_of_cols, raw_headers);
            let rows = parsed_cells_with_positions
                .into_iter()
                .map(|row| create_table_row(&buffer_snapshot, max_number_of_cols, row))
                .collect();
            let row_line_numbers = line_numbers.into_iter().skip(1).collect();
            (headers, rows, row_line_numbers)
        }
        HeaderMode::NoHeader => {
            let headers = create_synthetic_headers(max_number_of_cols);
            let rows = parsed_cells_with_positions
                .into_iter()
                .map(|row| create_table_row(&buffer_snapshot, max_number_of_cols, row))
                .collect();
            (headers, rows, line_numbers)
        }
    };

    TableLikeContent {
        headers,
        rows,
        line_numbers: row_line_numbers,
        number_of_cols: max_number_of_cols,
    }
}

fn create_synthetic_headers(max_number_of_cols: usize) -> TableRow<TableCell> {
    let headers = (1..=max_number_of_cols)
        .map(|column| TableCell::synthetic(column, format!("Column {column}")))
        .collect();
    TableRow::from_vec(headers, max_number_of_cols)
}

fn default_delimiter_for_extension(extension: &str) -> Option<char> {
    match extension {
        "tsv" => Some('\t'),
        "psv" => Some('|'),
        "scsv" | "ssv" => Some(';'),
        "csv" => Some(','),
        _ => None,
    }
}

fn has_separator_directive(text: &str) -> bool {
    text.lines().next().is_some_and(|line| {
        let line = line.trim_start();
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);
        let bytes = line.as_bytes();
        bytes.len() >= 5
            && line[..4].eq_ignore_ascii_case("sep=")
            && line[4..]
                .chars()
                .next()
                .is_some_and(|delimiter| delimiter != '\r' && delimiter != '\n' && delimiter != '"')
    })
}

fn separator_directive(text: &str) -> Option<char> {
    let first_line = text.lines().next()?.trim_start();
    let first_line = first_line.strip_prefix('\u{feff}').unwrap_or(first_line);
    if first_line.len() < 5 || !first_line[..4].eq_ignore_ascii_case("sep=") {
        return None;
    }
    let rest = &first_line[4..];
    let delimiter = rest.chars().next()?;
    (delimiter != '\r' && delimiter != '\n' && delimiter != '"').then_some(delimiter)
}

fn detect_delimiter(text: &str, fallback: char) -> (char, bool) {
    if let Some(delimiter) = separator_directive(text) {
        return (delimiter, true);
    }

    let candidates = [',', '\t', '|', ';'];
    let mut best = (fallback, 0usize);
    for candidate in candidates {
        let (rows, _) = parse_delimited_with_positions(text, candidate);
        let sample = rows.into_iter().take(32).collect::<Vec<_>>();
        let multi_column_rows = sample.iter().filter(|row| row.len() > 1).count();
        if multi_column_rows == 0 {
            continue;
        }
        let width = sample
            .iter()
            .filter(|row| row.len() > 1)
            .map(Vec::len)
            .max()
            .unwrap_or_default();
        let stable_rows = sample.iter().filter(|row| row.len() == width).count();
        let score = stable_rows * 100 + multi_column_rows * 10 + width;
        if score > best.1 {
            best = (candidate, score);
        }
    }
    (best.0, false)
}

/// Parse delimited text and track byte positions for each cell.
fn parse_delimited_with_positions(
    text: &str,
    delimiter: char,
) -> (
    Vec<Vec<(SharedString, std::ops::Range<usize>)>>,
    Vec<LineNumber>,
) {
    let mut rows = Vec::new();
    let mut line_numbers = Vec::new();
    let mut current_row: Vec<(SharedString, std::ops::Range<usize>)> = Vec::new();
    let mut current_field = String::new();
    let mut field_start_offset = 0;
    let mut current_offset = 0;
    let mut in_quotes = false;
    let mut current_line = 1; // 1-based line numbering
    let mut row_start_line = 1;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let char_byte_len = ch.len_utf8();

        match ch {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        // Escaped quote
                        chars.next();
                        current_field.push('"');
                        current_offset += 1; // Skip the second quote
                    } else {
                        // End of quoted field
                        in_quotes = false;
                    }
                } else {
                    // Start of quoted field
                    in_quotes = true;
                    if current_field.is_empty() {
                        // Include the opening quote in the range
                        field_start_offset = current_offset;
                    }
                }
            }
            separator if separator == delimiter && !in_quotes => {
                // Field separator
                let field_end_offset = current_offset;
                if current_field.is_empty() && !in_quotes {
                    field_start_offset = current_offset;
                }
                current_row.push((
                    current_field.clone().into(),
                    field_start_offset..field_end_offset,
                ));
                current_field.clear();
                field_start_offset = current_offset + char_byte_len;
            }
            '\n' => {
                if in_quotes {
                    // Newline inside quotes - preserve it
                    current_field.push(ch);
                } else {
                    current_line += 1;
                    // Row separator (only when not inside quotes)
                    let field_end_offset = current_offset;
                    if current_field.is_empty() && current_row.is_empty() {
                        field_start_offset = 0;
                    }
                    current_row.push((
                        current_field.clone().into(),
                        field_start_offset..field_end_offset,
                    ));
                    current_field.clear();

                    // Only add non-empty rows
                    if !current_row.is_empty()
                        && !current_row.iter().all(|(field, _)| field.trim().is_empty())
                    {
                        rows.push(current_row);
                        // Add line number info for this row
                        let line_info = if row_start_line == current_line - 1 {
                            LineNumber::Line(row_start_line)
                        } else {
                            LineNumber::LineRange(row_start_line, current_line - 1)
                        };
                        line_numbers.push(line_info);
                    }
                    current_row = Vec::new();
                    row_start_line = current_line;
                    field_start_offset = current_offset + char_byte_len;
                }
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    // Handle Windows line endings (\r\n): account for \r byte, let \n be handled next
                    current_offset += char_byte_len;
                    continue;
                }

                if in_quotes {
                    // \r inside quotes - preserve it
                    current_field.push(ch);
                } else {
                    // Standalone \r row separator
                    current_line += 1;
                    // Row separator (only when not inside quotes)
                    let field_end_offset = current_offset;
                    current_row.push((
                        current_field.clone().into(),
                        field_start_offset..field_end_offset,
                    ));
                    current_field.clear();

                    // Only add non-empty rows
                    if !current_row.is_empty()
                        && !current_row.iter().all(|(field, _)| field.trim().is_empty())
                    {
                        rows.push(current_row);
                        // Add line number info for this row
                        let line_info = if row_start_line == current_line - 1 {
                            LineNumber::Line(row_start_line)
                        } else {
                            LineNumber::LineRange(row_start_line, current_line - 1)
                        };
                        line_numbers.push(line_info);
                    }
                    current_row = Vec::new();
                    row_start_line = current_line;
                    field_start_offset = current_offset + char_byte_len;
                }
            }
            _ => {
                if current_field.is_empty() && !in_quotes {
                    field_start_offset = current_offset;
                }
                current_field.push(ch);
            }
        }

        current_offset += char_byte_len;
    }

    // Add the last field and row if not empty
    if !current_field.is_empty() || !current_row.is_empty() {
        let field_end_offset = current_offset;
        current_row.push((
            current_field.clone().into(),
            field_start_offset..field_end_offset,
        ));
    }
    if !current_row.is_empty() && !current_row.iter().all(|(field, _)| field.trim().is_empty()) {
        rows.push(current_row);
        // Add line number info for the last row
        let line_info = if row_start_line == current_line {
            LineNumber::Line(row_start_line)
        } else {
            LineNumber::LineRange(row_start_line, current_line)
        };
        line_numbers.push(line_info);
    }

    (rows, line_numbers)
}

fn create_table_row(
    buffer_snapshot: &BufferSnapshot,
    max_number_of_cols: usize,
    row: Vec<(SharedString, std::ops::Range<usize>)>,
) -> TableRow<TableCell> {
    let mut raw_row = row
        .into_iter()
        .map(|(content, range)| {
            TableCell::from_buffer_position(content, range.start, range.end, &buffer_snapshot)
        })
        .collect::<Vec<_>>();

    let append_elements = max_number_of_cols - raw_row.len();
    if append_elements > 0 {
        for _ in 0..append_elements {
            raw_row.push(TableCell::Virtual);
        }
    }

    TableRow::from_vec(raw_row, max_number_of_cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_parsing_basic() {
        let csv_data = "Name,Age,City\nJohn,30,New York\nJane,25,Los Angeles";
        let parsed = TableLikeContent::from_str(csv_data.to_string());

        assert_eq!(parsed.headers.cols(), 3);
        assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "Name");
        assert_eq!(parsed.headers[1].display_value().unwrap().as_ref(), "Age");
        assert_eq!(parsed.headers[2].display_value().unwrap().as_ref(), "City");

        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[0][0].display_value().unwrap().as_ref(), "John");
        assert_eq!(parsed.rows[0][1].display_value().unwrap().as_ref(), "30");
        assert_eq!(
            parsed.rows[0][2].display_value().unwrap().as_ref(),
            "New York"
        );
    }

    #[test]
    fn test_csv_parsing_with_quotes() {
        let csv_data = r#"Name,Description
"John Doe","A person with ""special"" characters"
Jane,"Simple name""#;
        let parsed = TableLikeContent::from_str(csv_data.to_string());

        assert_eq!(parsed.headers.cols(), 2);
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(
            parsed.rows[0][1].display_value().unwrap().as_ref(),
            r#"A person with "special" characters"#
        );
    }

    #[test]
    fn test_csv_parsing_with_newlines_in_quotes() {
        let csv_data = "Name,Description,Status\n\"John\nDoe\",\"A person with\nmultiple lines\",Active\n\"Jane Smith\",\"Simple\",\"Also\nActive\"";
        let parsed = TableLikeContent::from_str(csv_data.to_string());

        assert_eq!(parsed.headers.cols(), 3);
        assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "Name");
        assert_eq!(
            parsed.headers[1].display_value().unwrap().as_ref(),
            "Description"
        );
        assert_eq!(
            parsed.headers[2].display_value().unwrap().as_ref(),
            "Status"
        );

        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(
            parsed.rows[0][0].display_value().unwrap().as_ref(),
            "John\nDoe"
        );
        assert_eq!(
            parsed.rows[0][1].display_value().unwrap().as_ref(),
            "A person with\nmultiple lines"
        );
        assert_eq!(
            parsed.rows[0][2].display_value().unwrap().as_ref(),
            "Active"
        );

        assert_eq!(
            parsed.rows[1][0].display_value().unwrap().as_ref(),
            "Jane Smith"
        );
        assert_eq!(
            parsed.rows[1][1].display_value().unwrap().as_ref(),
            "Simple"
        );
        assert_eq!(
            parsed.rows[1][2].display_value().unwrap().as_ref(),
            "Also\nActive"
        );

        // Check line numbers
        assert_eq!(parsed.line_numbers.len(), 2);
        match &parsed.line_numbers[0] {
            LineNumber::Line(line) => {
                assert_eq!(line, &2);
            }
            _ => panic!("Expected logical row number for multiline row"),
        }
        match &parsed.line_numbers[1] {
            LineNumber::Line(line) => {
                assert_eq!(line, &3);
            }
            _ => panic!("Expected logical row number for second multiline row"),
        }
    }

    #[test]
    fn test_csv_parsing_rfc4180_fixture_uses_logical_row_numbers() {
        let csv_data = include_str!("../../grammars/src/csv/testdata/rfc4180.csv");
        let parsed = TableLikeContent::from_str(csv_data.to_string());

        let logical_rows = parsed
            .line_numbers
            .iter()
            .map(|line_number| match line_number {
                LineNumber::Line(line) => *line,
                LineNumber::LineRange(start, end) => {
                    panic!("expected logical line numbers only, got range {start}-{end}")
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(logical_rows, vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_tsv_parsing_reuses_delimited_preview_parser() {
        let tsv_data = "Name\tDescription\tStatus\nAlice\t\"contains, commas\"\tactive\nBob\t\"multiple\nlines\"\tinactive";
        let parsed = TableLikeContent::from_delimited_str(tsv_data.to_string(), '\t');

        assert_eq!(parsed.headers.cols(), 3);
        assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "Name");
        assert_eq!(
            parsed.rows[0][1].display_value().unwrap().as_ref(),
            "contains, commas"
        );
        assert_eq!(
            parsed.rows[1][1].display_value().unwrap().as_ref(),
            "multiple\nlines"
        );
        let logical_rows = parsed
            .line_numbers
            .iter()
            .map(|line_number| match line_number {
                LineNumber::Line(line) => *line,
                LineNumber::LineRange(start, end) => {
                    panic!("expected logical line number, got range {start}-{end}")
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(logical_rows, vec![2, 3]);
    }

    #[test]
    fn test_auto_detection_supports_psv_and_semicolon_files() {
        use text::{Buffer, BufferId, ReplicaId};

        let psv_buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(4).unwrap(),
            "name|value\na|1".to_string(),
        );
        let psv = from_buffer_with_selection(
            psv_buffer.snapshot(),
            DelimiterSelection::Auto,
            ',',
            HeaderMode::FirstRow,
        );
        assert_eq!(psv.headers.cols(), 2);
        assert_eq!(psv.rows[0][1].display_value().unwrap().as_ref(), "1");

        let semicolon_buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(5).unwrap(),
            "name;value\na;1".to_string(),
        );
        let semicolon = from_buffer_with_selection(
            semicolon_buffer.snapshot(),
            DelimiterSelection::Auto,
            ',',
            HeaderMode::FirstRow,
        );
        assert_eq!(semicolon.headers.cols(), 2);
        assert_eq!(semicolon.rows[0][1].display_value().unwrap().as_ref(), "1");

        let tsv_buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(6).unwrap(),
            "field,value\nalice,1".to_string(),
        );
        let tsv = from_buffer_with_selection(
            tsv_buffer.snapshot(),
            DelimiterSelection::Auto,
            '\t',
            HeaderMode::FirstRow,
        );
        assert_eq!(tsv.headers.cols(), 1);
        assert_eq!(tsv.rows[0][0].display_value().unwrap().as_ref(), "alice,1");
    }

    #[test]
    fn test_no_header_generates_column_names_and_keeps_first_row() {
        use text::{Buffer, BufferId, ReplicaId};

        let source = "alice,30\nbob,40".to_string();
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(2).unwrap(), source);
        let parsed = from_buffer_with_options(buffer.snapshot(), ',', HeaderMode::NoHeader, false);

        assert_eq!(
            parsed.headers[0].display_value().unwrap().as_ref(),
            "Column 1"
        );
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.line_numbers.len(), 2);
        assert_eq!(parsed.rows[0][0].display_value().unwrap().as_ref(), "alice");
    }

    #[test]
    fn test_separator_directive_is_hidden_from_preview() {
        use text::{Buffer, BufferId, ReplicaId};

        let source = "sep=;\nname;value\na;1".to_string();
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(3).unwrap(), source);
        let parsed = from_buffer_with_selection(
            buffer.snapshot(),
            DelimiterSelection::Auto,
            ',',
            HeaderMode::FirstRow,
        );

        assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "name");
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0][1].display_value().unwrap().as_ref(), "1");
    }

    #[test]
    fn test_empty_csv() {
        let parsed = TableLikeContent::from_str(String::new());
        assert_eq!(parsed.headers.cols(), 0);
        assert!(parsed.rows.is_empty());
    }

    #[test]
    fn test_csv_parsing_quote_offset_handling() {
        let csv_data = r#"first,"se,cond",third"#;
        let (parsed_cells, _) = parse_delimited_with_positions(csv_data, ',');

        assert_eq!(parsed_cells.len(), 1); // One row
        assert_eq!(parsed_cells[0].len(), 3); // Three cells

        // first: 0..5 (no quotes)
        let (content1, range1) = &parsed_cells[0][0];
        assert_eq!(content1.as_ref(), "first");
        assert_eq!(*range1, 0..5);

        // "se,cond": 6..15 (includes quotes in range, content without quotes)
        let (content2, range2) = &parsed_cells[0][1];
        assert_eq!(content2.as_ref(), "se,cond");
        assert_eq!(*range2, 6..15);

        // third: 16..21 (no quotes)
        let (content3, range3) = &parsed_cells[0][2];
        assert_eq!(content3.as_ref(), "third");
        assert_eq!(*range3, 16..21);
    }

    #[test]
    fn test_csv_parsing_complex_quotes() {
        let csv_data = r#"id,"name with spaces","description, with commas",status
1,"John Doe","A person with ""quotes"" and, commas",active
2,"Jane Smith","Simple description",inactive"#;
        let (parsed_cells, _) = parse_delimited_with_positions(csv_data, ',');

        assert_eq!(parsed_cells.len(), 3); // header + 2 rows

        // Check header row
        let header_row = &parsed_cells[0];
        assert_eq!(header_row.len(), 4);

        // id: 0..2
        assert_eq!(header_row[0].0.as_ref(), "id");
        assert_eq!(header_row[0].1, 0..2);

        // "name with spaces": 3..21 (includes quotes)
        assert_eq!(header_row[1].0.as_ref(), "name with spaces");
        assert_eq!(header_row[1].1, 3..21);

        // "description, with commas": 22..48 (includes quotes)
        assert_eq!(header_row[2].0.as_ref(), "description, with commas");
        assert_eq!(header_row[2].1, 22..48);

        // status: 49..55
        assert_eq!(header_row[3].0.as_ref(), "status");
        assert_eq!(header_row[3].1, 49..55);

        // Check first data row
        let first_row = &parsed_cells[1];
        assert_eq!(first_row.len(), 4);

        // 1: 56..57
        assert_eq!(first_row[0].0.as_ref(), "1");
        assert_eq!(first_row[0].1, 56..57);

        // "John Doe": 58..68 (includes quotes)
        assert_eq!(first_row[1].0.as_ref(), "John Doe");
        assert_eq!(first_row[1].1, 58..68);

        // Content should be stripped of quotes but include escaped quotes
        assert_eq!(
            first_row[2].0.as_ref(),
            r#"A person with "quotes" and, commas"#
        );
        // The range should include the outer quotes: 69..107
        assert_eq!(first_row[2].1, 69..107);

        // active: 108..114
        assert_eq!(first_row[3].0.as_ref(), "active");
        assert_eq!(first_row[3].1, 108..114);
    }
}

impl TableLikeContent {
    #[cfg(test)]
    pub fn from_str(text: String) -> Self {
        Self::from_delimited_str(text, ',')
    }

    #[cfg(test)]
    pub fn from_delimited_str(text: String, delimiter: char) -> Self {
        use text::{Buffer, BufferId, ReplicaId};

        let buffer_id = BufferId::new(1).unwrap();
        let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, text);
        let snapshot = buffer.snapshot();
        from_buffer(snapshot, delimiter)
    }
}
