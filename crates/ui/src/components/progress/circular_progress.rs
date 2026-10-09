use documented::Documented;
use gpui::{Hsla, PathBuilder, canvas, point};
use std::f32::consts::PI;

use crate::prelude::*;

use super::determinate_progress;

/// A circular progress indicator that displays progress as an arc growing clockwise from the top.
#[derive(IntoElement, RegisterComponent, Documented)]
pub struct CircularProgress {
    #[cfg(feature = "accessibility")]
    id: ElementId,
    #[cfg(feature = "accessibility")]
    accessibility_label: SharedString,
    value: f32,
    max_value: f32,
    size: Pixels,
    stroke_width: Pixels,
    bg_color: Hsla,
    progress_color: Hsla,
}

impl CircularProgress {
    /// Creates a determinate circular progress indicator with its accessible name.
    ///
    /// A semantic range is emitted only when `value` and `max_value` are finite and `max_value`
    /// is positive.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: f32,
        max_value: f32,
        size: Pixels,
        cx: &App,
    ) -> Self {
        #[cfg(not(feature = "accessibility"))]
        let _ = (id, label);

        Self {
            #[cfg(feature = "accessibility")]
            id: id.into(),
            #[cfg(feature = "accessibility")]
            accessibility_label: label.into(),
            value,
            max_value,
            size,
            stroke_width: px(4.0),
            bg_color: cx.theme().colors().border_variant,
            progress_color: cx.theme().status().info,
        }
    }

    /// Sets the current progress value.
    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }

    /// Sets the maximum value for the progress indicator.
    pub fn max_value(mut self, max_value: f32) -> Self {
        self.max_value = max_value;
        self
    }

    /// Sets the size (diameter) of the circular progress indicator.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Sets the stroke width of the circular progress indicator.
    pub fn stroke_width(mut self, stroke_width: Pixels) -> Self {
        self.stroke_width = stroke_width;
        self
    }

    /// Sets the background circle color.
    pub fn bg_color(mut self, color: Hsla) -> Self {
        self.bg_color = color;
        self
    }

    /// Sets the progress arc color.
    pub fn progress_color(mut self, color: Hsla) -> Self {
        self.progress_color = color;
        self
    }
}

impl RenderOnce for CircularProgress {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let progress = determinate_progress(self.value, self.max_value);
        let progress_fraction = progress.map_or(0., |progress| progress.fraction);
        let size = self.size;
        let stroke_width = self.stroke_width;
        let bg_color = self.bg_color;
        let progress_color = self.progress_color;

        let canvas = canvas(
            |_, _, _| {},
            move |bounds, _, window, _cx| {
                let center_x = bounds.origin.x + bounds.size.width / 2.0;
                let center_y = bounds.origin.y + bounds.size.height / 2.0;

                let radius = (size / 2.0) - stroke_width;

                // Draw background circle (full 360 degrees)
                let mut bg_builder = PathBuilder::stroke(stroke_width);

                // Start at rightmost point
                bg_builder.move_to(point(center_x + radius, center_y));

                // Draw full circle using two 180-degree arcs
                bg_builder.arc_to(
                    point(radius, radius),
                    px(0.),
                    false,
                    true,
                    point(center_x - radius, center_y),
                );
                bg_builder.arc_to(
                    point(radius, radius),
                    px(0.),
                    false,
                    true,
                    point(center_x + radius, center_y),
                );
                bg_builder.close();

                if let Ok(path) = bg_builder.build() {
                    window.paint_path(path, bg_color);
                }

                // Draw progress arc if there's any progress
                if progress_fraction > 0.0 {
                    let mut progress_builder = PathBuilder::stroke(stroke_width);

                    // Handle 100% progress as a special case by drawing a full circle
                    if progress_fraction >= 0.999 {
                        // Start at rightmost point
                        progress_builder.move_to(point(center_x + radius, center_y));

                        // Draw full circle using two 180-degree arcs
                        progress_builder.arc_to(
                            point(radius, radius),
                            px(0.),
                            false,
                            true,
                            point(center_x - radius, center_y),
                        );
                        progress_builder.arc_to(
                            point(radius, radius),
                            px(0.),
                            false,
                            true,
                            point(center_x + radius, center_y),
                        );
                        progress_builder.close();
                    } else {
                        // Start at 12 o'clock (top) position
                        let start_x = center_x;
                        let start_y = center_y - radius;
                        progress_builder.move_to(point(start_x, start_y));

                        // Calculate the end point of the arc based on progress
                        // Progress sweeps clockwise from -90° (top)
                        let angle = -PI / 2.0 + (progress_fraction * 2.0 * PI);
                        let end_x = center_x + radius * angle.cos();
                        let end_y = center_y + radius * angle.sin();

                        // Use large_arc flag when progress > 0.5 (more than 180 degrees)
                        let large_arc = progress_fraction > 0.5;

                        progress_builder.arc_to(
                            point(radius, radius),
                            px(0.),
                            large_arc,
                            true, // sweep clockwise
                            point(end_x, end_y),
                        );
                    }

                    if let Ok(path) = progress_builder.build() {
                        window.paint_path(path, progress_color);
                    }
                }
            },
        )
        .size(size);

        #[cfg(feature = "accessibility")]
        if let Some(progress) = progress {
            return div()
                .id(self.id)
                .size(size)
                .role(gpui::accesskit::Role::ProgressIndicator)
                .aria_label(self.accessibility_label)
                .aria_min_numeric_value(0.)
                .aria_numeric_value(progress.current_value)
                .aria_max_numeric_value(progress.max_value)
                .child(canvas)
                .into_any_element();
        }

        canvas.into_any_element()
    }
}

impl Component for CircularProgress {
    fn scope() -> ComponentScope {
        ComponentScope::Status
    }

    fn description() -> Option<&'static str> {
        Some(
            "A circular progress indicator that displays progress as an arc growing clockwise from the top.",
        )
    }

    fn preview(_window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let max_value = 100.0;
        let container = || v_flex().items_center().gap_1();

        Some(
            example_group(vec![single_example(
                "Examples",
                h_flex()
                    .gap_6()
                    .child(
                        container()
                            .child(CircularProgress::new(
                                "empty",
                                "Empty",
                                0.0,
                                max_value,
                                px(48.0),
                                cx,
                            ))
                            .child(Label::new("0%").size(LabelSize::Small)),
                    )
                    .child(
                        container()
                            .child(CircularProgress::new(
                                "quarter",
                                "25%",
                                25.0,
                                max_value,
                                px(48.0),
                                cx,
                            ))
                            .child(Label::new("25%").size(LabelSize::Small)),
                    )
                    .child(
                        container()
                            .child(CircularProgress::new(
                                "half",
                                "50%",
                                50.0,
                                max_value,
                                px(48.0),
                                cx,
                            ))
                            .child(Label::new("50%").size(LabelSize::Small)),
                    )
                    .child(
                        container()
                            .child(CircularProgress::new(
                                "three-quarters",
                                "75%",
                                75.0,
                                max_value,
                                px(48.0),
                                cx,
                            ))
                            .child(Label::new("75%").size(LabelSize::Small)),
                    )
                    .child(
                        container()
                            .child(CircularProgress::new(
                                "complete",
                                "Complete",
                                100.0,
                                max_value,
                                px(48.0),
                                cx,
                            ))
                            .child(Label::new("100%").size(LabelSize::Small)),
                    )
                    .into_any_element(),
            )])
            .into_any_element(),
        )
    }
}
