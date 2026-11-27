use anyhow::Context as _;
use futures::StreamExt;
use gpui::{App, AppContext as _, SerializedThreadTaskTimings};
use i18n::tr;
use log::info;
use memory_report::MemoryReporter;
use std::{
    io::{BufWriter, Write as _},
    path::Path,
    thread::ThreadId,
    time::Duration,
};
use util::ResultExt;
use workspace::{
    notifications::{NotificationId, simple_message_notification::MessageNotification},
    with_active_or_new_workspace,
};

use crate::STARTUP_TIME;

const MAX_HANG_TRACES: usize = 3;
const MAX_HANG_TRACE_TIMINGS_PER_THREAD: usize = 16 * 1024;

/// Seconds between memory samples in the log. Unset leaves sampling off, so a
/// shipped build pays nothing unless someone asks for the series.
const MEMORY_LOG_INTERVAL_VARIABLE: &str = "ZZZ_MEMORY_LOG_INTERVAL_SECS";

pub fn init(cx: &mut App) {
    if cfg!(debug_assertions) {
        log::info!("Debug assertions enabled, skipping hang monitoring");
    } else {
        monitor_hangs(cx);
    }

    monitor_memory(cx);
    monitor_idle_heap(cx);
    cx.on_action(|_: &zzz_actions::ReportMemoryUsage, cx| {
        let report = match memory_report::MemoryReport::current() {
            Ok(report) => report,
            Err(error) => {
                log::error!("could not sample memory usage: {error:#}");
                return;
            }
        };
        info!("memory usage: {report}");

        // The size distribution separates "many small objects retained" from "a
        // few large buffers retained", which the totals above cannot. It walks
        // every live block in this thread's heap, so it stays on this explicit
        // action and writes to the log rather than the notification.
        if let Some(buckets) = memory_report::block_size_distribution() {
            let distribution = buckets
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            info!("live heap blocks in this thread: {distribution}");
        }

        with_active_or_new_workspace(cx, |workspace, _window, cx| {
            // Distinct type so the notification replaces the previous sample
            // rather than stacking one per invocation.
            struct MemoryUsageSample;

            workspace.show_notification(
                NotificationId::unique::<MemoryUsageSample>(),
                cx,
                move |cx| {
                    let message = tr(cx, "zzz.memory_report.notification", "Memory usage: {}")
                        .replacen("{}", &report.to_string(), 1);
                    cx.new(|cx| MessageNotification::new(message, cx))
                },
            );
        });
    });
}

/// Logs a memory sample on an interval when
/// [`MEMORY_LOG_INTERVAL_VARIABLE`] is set.
///
/// A single sample cannot separate a leak from a large baseline; a series can,
/// which is why this runs on a timer rather than only on demand.
fn monitor_memory(cx: &App) {
    let Some(interval) = memory_log_interval() else {
        return;
    };

    let executor = cx.background_executor().clone();
    executor
        .spawn({
            let executor = executor.clone();
            async move {
                let mut reporter = match MemoryReporter::new() {
                    Ok(reporter) => reporter,
                    Err(error) => {
                        log::error!("could not start memory sampling: {error:#}");
                        return;
                    }
                };
                loop {
                    executor.timer(interval).await;
                    match reporter.report() {
                        Ok(report) => info!("memory usage: {report}"),
                        Err(error) => log::error!("could not sample memory usage: {error:#}"),
                    }
                }
            }
        })
        .detach();
}

fn memory_log_interval() -> Option<Duration> {
    let value = match std::env::var(MEMORY_LOG_INTERVAL_VARIABLE) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return None,
        Err(error @ std::env::VarError::NotUnicode(_)) => {
            log::warn!(
                "{MEMORY_LOG_INTERVAL_VARIABLE} is unusable, memory sampling stays off: {error}"
            );
            return None;
        }
    };
    match parse_memory_log_interval(&value) {
        Ok(interval) => interval,
        Err(error) => {
            log::warn!(
                "{MEMORY_LOG_INTERVAL_VARIABLE} is unusable, memory sampling stays off: {error:#}"
            );
            None
        }
    }
}

/// Parses the configured interval. Zero and negative values turn sampling off,
/// so a launcher can set the variable unconditionally.
fn parse_memory_log_interval(value: &str) -> anyhow::Result<Option<Duration>> {
    let seconds: i64 = value
        .trim()
        .parse()
        .with_context(|| format!("{value:?} is not a whole number of seconds"))?;
    Ok((seconds > 0).then(|| Duration::from_secs(seconds.unsigned_abs())))
}

/// Sampling cadence for the idle check. Independent of the logging interval:
/// this task runs even when logging is off, because it decides when to return
/// memory rather than whether to report it.
const IDLE_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);

/// How many consecutive unchanged samples make the process count as idle. Three
/// repeats after the baseline is about two minutes at [`IDLE_SAMPLE_INTERVAL`].
const IDLE_SAMPLES_BEFORE_RELEASE: u32 = 3;

/// Set to `0` to keep unused heap pages instead of returning them when idle.
const IDLE_RELEASE_VARIABLE: &str = "ZZZ_MEMORY_IDLE_RELEASE";

/// Returns unused heap pages to the OS once the process goes quiet.
///
/// mimalloc purges on a delay that new allocations keep pushing back, so an
/// editor left open on a large project keeps its peak working set resident long
/// after the project is closed. Collecting only once memory has stopped moving
/// keeps the cost off every allocation path.
fn monitor_idle_heap(cx: &App) {
    if !idle_release_enabled() {
        return;
    }

    let executor = cx.background_executor().clone();
    executor
        .spawn({
            let executor = executor.clone();
            async move {
                let mut reporter = match MemoryReporter::new() {
                    Ok(reporter) => reporter,
                    Err(error) => {
                        log::error!("could not start idle memory release: {error:#}");
                        return;
                    }
                };
                let mut collector = IdleHeapCollector::default();
                loop {
                    executor.timer(IDLE_SAMPLE_INTERVAL).await;
                    let report = match reporter.report() {
                        Ok(report) => report,
                        Err(error) => {
                            log::debug!("could not sample memory for idle release: {error:#}");
                            continue;
                        }
                    };
                    let committed_bytes = report
                        .allocator
                        .as_ref()
                        .map_or(0, |allocator| allocator.committed_bytes);
                    if collector.observe(report.resident_bytes, committed_bytes) {
                        memory_report::release_unused_memory();
                        info!(
                            "{IDLE_RELEASE_VARIABLE}: released unused heap pages after \
                             {IDLE_SAMPLES_BEFORE_RELEASE} unchanged samples"
                        );
                    }
                }
            }
        })
        .detach();
}

fn idle_release_enabled() -> bool {
    match std::env::var(IDLE_RELEASE_VARIABLE) {
        Ok(value) => !matches!(value.trim(), "0" | "false" | "off"),
        Err(std::env::VarError::NotPresent) => true,
        Err(error) => {
            log::warn!("{IDLE_RELEASE_VARIABLE} is unreadable, keeping unused heap pages: {error}");
            false
        }
    }
}

/// Decides when a quiet process should hand unused heap back to the OS.
///
/// Moving memory means work is happening, and a collection then pays for itself
/// out of the next allocation burst; only an unchanged run of samples says the
/// pages really are idle. One collection per quiet period is enough, so a later
/// burst re-arms it instead of collecting on every interval.
#[derive(Debug, Default)]
struct IdleHeapCollector {
    previous: Option<(u64, u64)>,
    quiet_samples: u32,
    released: bool,
}

impl IdleHeapCollector {
    /// Records a sample and returns `true` for the sample that should release
    /// unused memory.
    fn observe(&mut self, resident_bytes: u64, committed_bytes: u64) -> bool {
        let current = (resident_bytes, committed_bytes);
        if self.previous == Some(current) {
            self.quiet_samples += 1;
        } else {
            self.previous = Some(current);
            self.quiet_samples = 0;
            self.released = false;
            return false;
        }

        if self.quiet_samples >= IDLE_SAMPLES_BEFORE_RELEASE && !self.released {
            self.released = true;
            return true;
        }
        false
    }
}

fn monitor_hangs(cx: &App) {
    let main_thread_id = std::thread::current().id();

    let foreground_executor = cx.foreground_executor();
    let background_executor = cx.background_executor();

    // 3 seconds hang
    let (mut tx, mut rx) = futures::channel::mpsc::channel(3);
    foreground_executor
        .spawn(async move { while (rx.next().await).is_some() {} })
        .detach();

    background_executor
        .spawn({
            let background_executor = background_executor.clone();
            async move {
                cleanup_old_hang_traces();

                let mut hanging = false;
                loop {
                    background_executor.timer(Duration::from_secs(1)).await;
                    match tx.try_send(()) {
                        Ok(_) => {
                            hanging = false;
                        }
                        Err(e) => {
                            let is_full = e.into_send_error().is_full();
                            if is_full && !hanging {
                                hanging = true;
                                save_hang_trace(
                                    main_thread_id,
                                    &background_executor,
                                    jiff::ZonedDateTime::now(),
                                );
                            }
                        }
                    }
                }
            }
        })
        .detach();
}

fn cleanup_old_hang_traces() {
    prune_hang_traces(paths::hang_traces_dir(), MAX_HANG_TRACES);
}

fn prune_hang_traces(directory: &Path, maximum_count: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.log_err())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json" || extension == "miniprof")
        })
        .collect();
    let number_to_remove = files.len().saturating_sub(maximum_count);
    if number_to_remove == 0 {
        return;
    }

    files.sort_by_key(|entry| entry.file_name());
    for entry in files.iter().take(number_to_remove) {
        std::fs::remove_file(entry.path()).log_err();
    }
}

fn save_hang_trace(
    main_thread_id: ThreadId,
    background_executor: &gpui::BackgroundExecutor,
    hang_time: jiff::ZonedDateTime,
) {
    let thread_timings = background_executor
        .dispatcher()
        .get_recent_timings(MAX_HANG_TRACE_TIMINGS_PER_THREAD);
    let thread_timings = thread_timings
        .into_iter()
        .map(|mut timings| {
            if timings.thread_id == main_thread_id {
                timings.thread_name = Some("main".to_owned());
            }

            SerializedThreadTaskTimings::convert(
                *STARTUP_TIME.get().expect("entry should be present"),
                timings,
            )
        })
        .collect::<Vec<_>>();

    let trace_path = paths::hang_traces_dir().join(&format!(
        "hang-{}.miniprof.json",
        hang_time.strftime("%Y-%m-%d_%H-%M-%S")
    ));

    prune_hang_traces(paths::hang_traces_dir(), MAX_HANG_TRACES - 1);

    if write_hang_trace(&trace_path, &thread_timings)
        .log_err()
        .is_none()
    {
        return;
    }

    info!(
        "hang detected, trace file saved at: {}",
        trace_path.display()
    );
}

fn write_hang_trace(
    path: &Path,
    thread_timings: &[SerializedThreadTaskTimings],
) -> anyhow::Result<()> {
    let file = std::fs::File::create(path).context("hang trace file creation")?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, thread_timings).context("hang timings serialization")?;
    writer.flush().context("hang trace file writing")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_memory_log_interval_accepts_positive_seconds() {
        assert_eq!(
            parse_memory_log_interval("60").expect("valid interval"),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            parse_memory_log_interval(" 5 ").expect("valid interval"),
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn parse_memory_log_interval_treats_zero_and_negative_as_off() {
        // A launcher can set the variable unconditionally and still disable
        // sampling.
        assert_eq!(
            parse_memory_log_interval("0").expect("valid interval"),
            None
        );
        assert_eq!(
            parse_memory_log_interval("-30").expect("valid interval"),
            None
        );
    }

    #[test]
    fn parse_memory_log_interval_rejects_non_numeric_values() {
        assert!(parse_memory_log_interval("60s").is_err());
        assert!(parse_memory_log_interval("").is_err());
    }

    #[test]
    fn idle_collector_releases_once_per_quiet_period() {
        let mut collector = IdleHeapCollector::default();
        assert!(
            !collector.observe(100, 50),
            "the baseline sample is not idle"
        );

        assert!(!collector.observe(100, 50), "one repeat is not enough");
        assert!(!collector.observe(100, 50));
        assert!(
            collector.observe(100, 50),
            "the third unchanged sample releases"
        );
        assert!(
            !collector.observe(100, 50),
            "a second release in the same quiet period would run the collection for nothing"
        );
    }

    #[test]
    fn idle_collector_ignores_a_heap_that_keeps_moving() {
        let mut collector = IdleHeapCollector::default();
        for byte in 0..(IDLE_SAMPLES_BEFORE_RELEASE as u64 + 4) {
            assert!(
                !collector.observe(100 + byte, 50 + byte),
                "growing memory is work in progress, not idle"
            );
        }
    }

    #[test]
    fn idle_collector_rearms_after_the_heap_changes() {
        let mut collector = IdleHeapCollector::default();
        assert!(!collector.observe(100, 50));
        for _ in 0..IDLE_SAMPLES_BEFORE_RELEASE {
            collector.observe(100, 50);
        }
        assert!(!collector.observe(100, 50), "already released");

        // A burst of work re-arms the decision.
        assert!(!collector.observe(200, 120));
        assert!(!collector.observe(200, 120));
        assert!(!collector.observe(200, 120));
        assert!(
            collector.observe(200, 120),
            "a new quiet period releases again"
        );
    }

    #[test]
    fn prune_hang_traces_keeps_the_newest_trace_files() {
        let directory =
            std::env::temp_dir().join(format!("zzz-hang-traces-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        for file_name in [
            "hang-2026-01-01.json",
            "hang-2026-01-02.miniprof",
            "hang-2026-01-03.miniprof.json",
            "not-a-trace.txt",
        ] {
            std::fs::File::create(directory.join(file_name)).unwrap();
        }

        prune_hang_traces(&directory, 2);

        assert!(!directory.join("hang-2026-01-01.json").exists());
        assert!(directory.join("hang-2026-01-02.miniprof").exists());
        assert!(directory.join("hang-2026-01-03.miniprof.json").exists());
        assert!(directory.join("not-a-trace.txt").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
