//! Rendering for the terminal-mode server monitor.
//!
//! The monitor is a compact section that lives at the bottom of the session file
//! panel, echoing FinalShell's per-session resource dashboard: host identity,
//! CPU, memory, swap, disk, load and network throughput.

use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled};
use i18n::tr;
use ui::{Color, Label, LabelSize, ProgressBar, prelude::*};

use crate::metrics::{ServerMetrics, format_bytes, format_rate, format_uptime};

/// Renders the monitor section for the active session.
///
/// `metrics` is `None` while the first sample is still in flight or when the
/// remote host does not expose the expected `/proc` files.
pub fn render_monitor(metrics: Option<&ServerMetrics>, enabled: bool, cx: &App) -> AnyElement {
    let mut column = v_flex()
        .w_full()
        .gap_2()
        .p_2()
        .border_t_1()
        .border_color(cx.theme().colors().border)
        .child(
            Label::new(tr(cx, "terminal_mode.monitor.title", "Server Monitor"))
                .size(LabelSize::Small),
        );

    if !enabled {
        return column
            .child(
                Label::new(tr(
                    cx,
                    "terminal_mode.monitor.disabled",
                    "Monitoring is disabled in settings",
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .into_any_element();
    }

    let Some(metrics) = metrics else {
        return column
            .child(
                Label::new(tr(
                    cx,
                    "terminal_mode.monitor.waiting",
                    "Collecting metrics…",
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .into_any_element();
    };

    let mut overview = h_flex().flex_wrap().gap_2();
    if let Some(hostname) = metrics.hostname.as_ref() {
        overview = overview.child(Label::new(hostname.clone()).size(LabelSize::Small));
    }
    if let Some(os) = metrics.os.as_ref() {
        overview = overview.child(
            Label::new(os.clone())
                .size(LabelSize::Small)
                .color(Color::Muted),
        );
    }
    if let Some(uptime) = metrics.uptime_secs {
        overview = overview.child(
            Label::new(format!(
                "{} {}",
                tr(cx, "terminal_mode.monitor.uptime", "Uptime"),
                format_uptime(uptime),
            ))
            .size(LabelSize::Small)
            .color(Color::Muted),
        );
    }
    column = column.child(overview);

    if let Some(cpu) = metrics.cpu_usage {
        column = column.child(metric_bar(
            "terminal-mode-monitor-cpu",
            tr(cx, "terminal_mode.monitor.cpu", "CPU"),
            cpu as f32,
            1.0,
            format!("{:.0}%", cpu * 100.0),
            cx,
        ));
    }

    if let Some(memory) = metrics.memory.as_ref() {
        column = column.child(metric_bar(
            "terminal-mode-monitor-memory",
            tr(cx, "terminal_mode.monitor.memory", "Memory"),
            memory.used_bytes as f32,
            memory.total_bytes as f32,
            format!(
                "{} / {}",
                format_bytes(memory.used_bytes),
                format_bytes(memory.total_bytes),
            ),
            cx,
        ));
    }

    if let Some(swap) = metrics.swap.as_ref()
        && swap.total_bytes > 0
    {
        column = column.child(metric_bar(
            "terminal-mode-monitor-swap",
            tr(cx, "terminal_mode.monitor.swap", "Swap"),
            swap.used_bytes as f32,
            swap.total_bytes as f32,
            format!(
                "{} / {}",
                format_bytes(swap.used_bytes),
                format_bytes(swap.total_bytes),
            ),
            cx,
        ));
    }

    if let Some(disk) = metrics.disks.first() {
        column = column.child(metric_bar(
            "terminal-mode-monitor-disk",
            tr(cx, "terminal_mode.monitor.disk", "Disk"),
            disk.used_bytes as f32,
            disk.total_bytes as f32,
            format!(
                "{} / {} ({})",
                format_bytes(disk.used_bytes),
                format_bytes(disk.total_bytes),
                disk.mount_point,
            ),
            cx,
        ));
    }

    if let Some(load) = metrics.load.as_ref() {
        column = column.child(
            Label::new(format!(
                "{} {:.2}  {:.2}  {:.2}",
                tr(cx, "terminal_mode.monitor.load", "Load"),
                load.one,
                load.five,
                load.fifteen,
            ))
            .size(LabelSize::Small)
            .color(Color::Muted),
        );
    }

    if !metrics.network.is_empty() {
        let received: f64 = metrics
            .network
            .iter()
            .map(|interface| interface.receive_bytes_per_sec)
            .sum();
        let transmitted: f64 = metrics
            .network
            .iter()
            .map(|interface| interface.transmit_bytes_per_sec)
            .sum();
        column = column.child(
            Label::new(format!(
                "↓ {}   ↑ {}",
                format_rate(received),
                format_rate(transmitted),
            ))
            .size(LabelSize::Small)
            .color(Color::Muted),
        );
    }

    column.into_any_element()
}

/// A labelled progress bar with the value and total shown on the right.
fn metric_bar(
    id: &'static str,
    label: String,
    value: f32,
    max_value: f32,
    detail: String,
    cx: &App,
) -> AnyElement {
    let fraction = if max_value > 0.0 {
        (value / max_value).clamp(0.0, 1.0)
    } else {
        0.0
    };
    v_flex()
        .w_full()
        .gap_1()
        .child(
            h_flex()
                .justify_between()
                .child(Label::new(label.clone()).size(LabelSize::Small))
                .child(
                    Label::new(detail)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(
            ProgressBar::new(id, label, value, max_value, cx).fg_color(usage_color(fraction, cx)),
        )
        .into_any_element()
}

/// Colors a usage bar by how close it is to saturation.
fn usage_color(fraction: f32, cx: &App) -> Hsla {
    let status = cx.theme().status();
    if fraction >= 0.9 {
        status.error
    } else if fraction >= 0.7 {
        status.warning
    } else {
        status.info
    }
}
