//! Performance metrics overlay for CSV preview debugging.
//!
//! Provides a semi-transparent overlay in the bottom-right corner showing
//! CSV parsing performance metrics for developer experience.

use i18n::tr;
use ui::{ActiveTheme, Context, IntoElement, ParentElement, Styled, StyledTypography, div};

use crate::{CsvPreviewView, PerformanceMetrics};

impl CsvPreviewView {
    /// Renders a semi-transparent performance metrics overlay in the bottom-right corner.
    ///
    /// Shows CSV parsing duration for debugging and performance monitoring.
    /// The overlay is positioned absolutely and styled with reduced opacity.
    pub(crate) fn render_performance_metrics_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let performance_metrics = tr(
            cx,
            "csv_preview.performance_metrics.title",
            "Performance metrics:",
        );
        let no_timing_data_yet = tr(
            cx,
            "csv_preview.performance_metrics.no_timing_data",
            "- No timing data yet",
        );
        let rendered_none = tr(
            cx,
            "csv_preview.performance_metrics.rendered_none",
            "- Rendered: none",
        );
        let rendered_rows = tr(
            cx,
            "csv_preview.performance_metrics.rendered_rows",
            "- Rendered: {} rows",
        );

        let children = div()
            .absolute()
            .top_24()
            .right_4()
            .px_3()
            .py_2()
            .bg(theme.colors().editor_background)
            .border_1()
            .border_color(theme.colors().border)
            .rounded_md()
            .opacity(0.75)
            .text_xs()
            .font_buffer(cx)
            .text_color(theme.colors().text_muted)
            .flex()
            .flex_col()
            .gap_1()
            .child(performance_metrics)
            .children(
                format_performance_metrics(
                    &self.performance_metrics,
                    &no_timing_data_yet,
                    &rendered_none,
                    &rendered_rows,
                )
                .into_iter()
                .map(|line| div().child(line)),
            );

        // Clear rendered indices to prepare for next frame
        self.performance_metrics.rendered_indices.clear();
        children
    }
}

fn format_performance_metrics(
    metrics: &PerformanceMetrics,
    no_timing_data_yet: &str,
    rendered_none: &str,
    rendered_rows: &str,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Add timing metrics using the display method
    let timing_display = metrics.display();
    if !timing_display.is_empty() {
        lines.extend(timing_display.lines().map(|line| format!("- {}", line)));
    } else {
        lines.push(no_timing_data_yet.to_owned());
    }

    // Add rendered indices information
    if metrics.rendered_indices.is_empty() {
        lines.push(rendered_none.to_owned());
    } else {
        lines.push(rendered_rows.replacen("{}", &metrics.rendered_indices.len().to_string(), 1));
        if metrics.rendered_indices.len() <= 20 {
            // Show indices if not too many
            lines.push(format!("  {:?}", metrics.rendered_indices));
        } else {
            // Show first/last few if too many
            let first_few = &metrics.rendered_indices[..5];
            let last_few = &metrics.rendered_indices[metrics.rendered_indices.len() - 5..];
            lines.push(format!("  {:?}\n..{:?}", first_few, last_few));
        }
    }

    lines
}
