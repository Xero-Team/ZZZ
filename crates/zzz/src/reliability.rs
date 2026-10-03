use anyhow::Context as _;
use futures::StreamExt;
use gpui::{App, SerializedThreadTaskTimings};
use log::info;
use std::{
    io::{BufWriter, Write as _},
    path::Path,
    thread::ThreadId,
    time::Duration,
};
use util::ResultExt;

use crate::STARTUP_TIME;

const MAX_HANG_TRACES: usize = 3;
const MAX_HANG_TRACE_TIMINGS_PER_THREAD: usize = 16 * 1024;

pub fn init(cx: &mut App) {
    if cfg!(debug_assertions) {
        log::info!("Debug assertions enabled, skipping hang monitoring");
    } else {
        monitor_hangs(cx);
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
                                    chrono::Local::now(),
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
    if let Ok(entries) = std::fs::read_dir(paths::hang_traces_dir()) {
        let mut files: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "json" || ext == "miniprof")
            })
            .collect();

        if files.len() > MAX_HANG_TRACES {
            files.sort_by_key(|entry| entry.file_name());
            for entry in files.iter().take(files.len() - MAX_HANG_TRACES) {
                std::fs::remove_file(entry.path()).log_err();
            }
        }
    }
}

fn save_hang_trace(
    main_thread_id: ThreadId,
    background_executor: &gpui::BackgroundExecutor,
    hang_time: chrono::DateTime<chrono::Local>,
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
        hang_time.format("%Y-%m-%d_%H-%M-%S")
    ));

    if let Ok(entries) = std::fs::read_dir(paths::hang_traces_dir()) {
        let mut files: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "json" || ext == "miniprof")
            })
            .collect();

        if files.len() >= MAX_HANG_TRACES {
            files.sort_by_key(|entry| entry.file_name());
            for entry in files.iter().take(files.len() - (MAX_HANG_TRACES - 1)) {
                std::fs::remove_file(entry.path()).log_err();
            }
        }
    }

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
