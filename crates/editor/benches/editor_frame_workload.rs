use std::{
    alloc::{GlobalAlloc, Layout, System},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

use anyhow::{Context as _, Result, bail};
use editor::{Editor, EditorMode, MultiBuffer};
#[cfg(feature = "frame-diagnostics")]
use gpui::profiler::{FrameDiagnosticsSnapshot, FrameEvent, FramePhase, FrameTimingCollector};
use gpui::{
    AnyWindowHandle, AppContext as _, Focusable as _, Keystroke, TestAppContext, TestDispatcher,
    WindowHandle, point, px, size,
};
use settings::SettingsStore;

const SEED: u64 = 0x5A5A_4750_5549_2026;
const WARMUP_FRAMES: usize = 20;
const CACHED_FRAMES: usize = 1_000;
const SCROLL_FRAMES: usize = 10 * 60;

struct CountingAllocator;

static COUNTING_ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The system allocator receives the layout supplied by Rust's allocation API.
        let pointer = unsafe { System.alloc(layout) };
        record_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The system allocator receives the layout supplied by Rust's allocation API.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_allocation(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The pointer and layout pair came from this allocator's allocation methods.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The pointer and layout pair came from this allocator's allocation methods.
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        record_allocation(pointer, new_size);
        pointer
    }
}

fn record_allocation(pointer: *mut u8, bytes: usize) {
    if !pointer.is_null() && COUNTING_ENABLED.load(Ordering::Relaxed) {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct WorkloadMeasurement {
    frames: usize,
    elapsed_nanoseconds: u128,
    allocations: usize,
    allocated_bytes: usize,
}

fn measure(frames: usize, workload: impl FnOnce()) -> WorkloadMeasurement {
    COUNTING_ENABLED.store(false, Ordering::SeqCst);
    ALLOCATION_COUNT.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    COUNTING_ENABLED.store(true, Ordering::SeqCst);
    let started_at = Instant::now();
    workload();
    let elapsed_nanoseconds = started_at.elapsed().as_nanos();
    COUNTING_ENABLED.store(false, Ordering::SeqCst);

    WorkloadMeasurement {
        frames,
        elapsed_nanoseconds,
        allocations: ALLOCATION_COUNT.load(Ordering::SeqCst),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst),
    }
}

fn report_measurement(name: &str, measurement: WorkloadMeasurement) {
    println!(
        "EDITOR_FRAME_WORKLOAD workload={name} frames={} elapsed_ns={} allocations={} allocated_bytes={} ns_per_frame={} allocations_per_frame={:.3} bytes_per_frame={:.3}",
        measurement.frames,
        measurement.elapsed_nanoseconds,
        measurement.allocations,
        measurement.allocated_bytes,
        measurement.elapsed_nanoseconds / measurement.frames as u128,
        measurement.allocations as f64 / measurement.frames as f64,
        measurement.allocated_bytes as f64 / measurement.frames as f64,
    );
}

#[cfg(feature = "frame-diagnostics")]
fn report_diagnostics(name: &str, snapshot: FrameDiagnosticsSnapshot) {
    let mut draw_durations = Vec::new();
    let mut input_latencies = Vec::new();
    let mut phase_durations = [0u128; 5];
    let mut phase_operations = [0u64; 5];
    let mut cache_hits = [0u64; 5];
    let mut invalidations = 0usize;
    let mut coalesced_invalidations = 0usize;
    let mut presented = 0usize;
    let mut skipped = 0usize;

    for event in snapshot.events {
        match event {
            FrameEvent::Invalidated(invalidation) => {
                invalidations += 1;
                coalesced_invalidations += usize::from(invalidation.coalesced);
            }
            FrameEvent::DrawFinished(timing) => {
                draw_durations.push(timing.draw_duration().as_nanos());
            }
            FrameEvent::Presented(timing) => {
                presented += 1;
                if let Some(latency) = timing.input_to_present {
                    input_latencies.push(latency.as_nanos());
                }
            }
            FrameEvent::Phase(timing) => {
                let index = match timing.phase {
                    FramePhase::RequestLayout => 0,
                    FramePhase::Prepaint => 1,
                    FramePhase::Paint => 2,
                    FramePhase::PrepaintCacheReplay => 3,
                    FramePhase::PaintCacheReplay => 4,
                };
                phase_durations[index] += timing.duration.as_nanos();
                phase_operations[index] += timing.operations;
                cache_hits[index] += timing.cache_hits;
            }
            FrameEvent::SubmissionSkipped { .. } => skipped += 1,
            FrameEvent::DrawStarted { .. } => {}
        }
    }

    println!(
        "EDITOR_FRAME_DIAGNOSTICS workload={name} dropped_events={} draws={} draw_p50_ns={} draw_p95_ns={} draw_p99_ns={} input_samples={} input_p50_ns={} input_p95_ns={} input_p99_ns={} invalidations={} coalesced_invalidations={} presented={} skipped={} phase_request_layout_ns={} phase_prepaint_ns={} phase_paint_ns={} phase_prepaint_replay_ns={} phase_paint_replay_ns={} phase_request_layout_ops={} phase_prepaint_ops={} phase_paint_ops={} prepaint_cache_hits={} paint_cache_hits={}",
        snapshot.dropped_events,
        draw_durations.len(),
        percentile(&mut draw_durations, 50),
        percentile(&mut draw_durations, 95),
        percentile(&mut draw_durations, 99),
        input_latencies.len(),
        percentile(&mut input_latencies, 50),
        percentile(&mut input_latencies, 95),
        percentile(&mut input_latencies, 99),
        invalidations,
        coalesced_invalidations,
        presented,
        skipped,
        phase_durations[0],
        phase_durations[1],
        phase_durations[2],
        phase_durations[3],
        phase_durations[4],
        phase_operations[0],
        phase_operations[1],
        phase_operations[2],
        cache_hits[3],
        cache_hits[4],
    );
}

#[cfg(feature = "frame-diagnostics")]
fn percentile(samples: &mut [u128], percentage: usize) -> u128 {
    samples.sort_unstable();
    let index = samples
        .len()
        .saturating_mul(percentage)
        .div_ceil(100)
        .saturating_sub(1);
    samples.get(index).copied().unwrap_or_default()
}

fn draw_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.draw_and_present_for_test(cx))
        .expect("benchmark window should remain open");
}

fn present_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.present_for_test())
        .expect("benchmark window should remain open");
}

fn read_resize_workload(path: &Path) -> Result<Vec<(f32, f32)>> {
    let input = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read resize workload {}", path.display()))?;
    input
        .lines()
        .skip(1)
        .map(|line| {
            let (width, height) = line
                .split_once('\t')
                .with_context(|| format!("invalid resize row: {line}"))?;
            Ok((
                width.parse().context("invalid resize width")?,
                height.parse().context("invalid resize height")?,
            ))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn peak_rss_kib() -> Result<Option<u64>> {
    let status =
        std::fs::read_to_string("/proc/self/status").context("failed to read /proc/self/status")?;
    Ok(status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?.trim();
        value.split_whitespace().next()?.parse().ok()
    }))
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_kib() -> Result<Option<u64>> {
    Ok(None)
}

fn resolve_input_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() || path.exists() {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }
}

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let source_path = resolve_input_path(
        arguments
            .next()
            .context("usage: editor_frame_workload <source> <typing> <resize-tsv>")?,
    );
    let typing_path = resolve_input_path(
        arguments
            .next()
            .context("usage: editor_frame_workload <source> <typing> <resize-tsv>")?,
    );
    let resize_path = resolve_input_path(
        arguments
            .next()
            .context("usage: editor_frame_workload <source> <typing> <resize-tsv>")?,
    );
    if arguments.any(|argument| argument.as_os_str() != "--bench") {
        bail!("usage: editor_frame_workload <source> <typing> <resize-tsv>");
    }

    let source = std::fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read source fixture {}", source_path.display()))?;
    let typing = std::fs::read_to_string(&typing_path)
        .with_context(|| format!("failed to read typing fixture {}", typing_path.display()))?;
    let resize_workload = read_resize_workload(&resize_path)?;
    let source_line_count = source
        .lines()
        .filter(|line| line.starts_with("pub fn line_"))
        .count();
    if source_line_count != 100_000 {
        bail!("source fixture must contain exactly 100,000 generated Rust lines");
    }
    if typing.chars().count() != 1_000 {
        bail!("typing fixture must contain exactly 1,000 characters");
    }
    if resize_workload.len() != 100 {
        bail!("resize workload must contain exactly 100 rows");
    }
    let typing_keystrokes = typing
        .chars()
        .map(|character| Keystroke::parse(character.encode_utf8(&mut [0; 4])))
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("typing fixture contains an invalid keystroke")?;
    println!(
        "EDITOR_FRAME_FIXTURE source_lines={source_line_count} source_bytes={} typed_characters={} resize_rows={} scroll_frames={SCROLL_FRAMES} seed={SEED:#x}",
        source.len(),
        typing.chars().count(),
        resize_workload.len(),
    );

    let mut cx = TestAppContext::build(TestDispatcher::new(SEED), Some("editor_frame_workload"));
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        assets::Assets.load_test_fonts(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });

    let buffer = cx.update(|cx| MultiBuffer::build_simple(&source, cx));
    let editor: WindowHandle<Editor> = cx.add_window(move |window, cx| {
        let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
        editor.set_style(editor::EditorStyle::default(), window, cx);
        editor
    });
    let window: AnyWindowHandle = editor.into();
    editor
        .update(&mut cx, |editor, window, cx| {
            window.focus(&editor.focus_handle(cx), cx)
        })
        .context("benchmark editor window should remain open")?;
    cx.run_until_parked();

    for _ in 0..WARMUP_FRAMES {
        draw_frame(&mut cx, window);
    }

    #[cfg(feature = "frame-diagnostics")]
    let mut collector = FrameTimingCollector::new();

    let cached = measure(CACHED_FRAMES, || {
        for _ in 0..CACHED_FRAMES {
            draw_frame(&mut cx, window);
        }
    });
    report_measurement("cached", cached);
    #[cfg(feature = "frame-diagnostics")]
    report_diagnostics("cached", collector.snapshot());
    cx.run_until_parked();
    #[cfg(feature = "frame-diagnostics")]
    collector.snapshot();

    let typed_characters = typing_keystrokes.len();
    let typing = measure(typed_characters, || {
        for keystroke in &typing_keystrokes {
            cx.dispatch_keystroke(window, keystroke.clone());
            present_frame(&mut cx, window);
        }
    });
    report_measurement("typing", typing);
    #[cfg(feature = "frame-diagnostics")]
    report_diagnostics("typing", collector.snapshot());
    cx.run_until_parked();
    #[cfg(feature = "frame-diagnostics")]
    collector.snapshot();

    let scrolling = measure(SCROLL_FRAMES, || {
        for frame in 0..SCROLL_FRAMES {
            let row = frame as f64 * 99_999.0 / (SCROLL_FRAMES - 1) as f64;
            editor
                .update(&mut cx, |editor, window, cx| {
                    editor.set_scroll_position(point(0.0, row), window, cx);
                })
                .expect("benchmark editor window should remain open");
            present_frame(&mut cx, window);
        }
    });
    report_measurement("scroll_10s_60hz", scrolling);
    #[cfg(feature = "frame-diagnostics")]
    report_diagnostics("scroll_10s_60hz", collector.snapshot());
    cx.run_until_parked();
    #[cfg(feature = "frame-diagnostics")]
    collector.snapshot();

    let resize_count = resize_workload.len();
    let resizing = measure(resize_count, || {
        for (width, height) in resize_workload {
            cx.simulate_window_resize(window, size(px(width), px(height)));
            present_frame(&mut cx, window);
        }
    });
    report_measurement("resize", resizing);
    #[cfg(feature = "frame-diagnostics")]
    report_diagnostics("resize", collector.snapshot());
    cx.run_until_parked();

    match peak_rss_kib()? {
        Some(peak_rss_kib) => println!("EDITOR_FRAME_RSS peak_kib={peak_rss_kib}"),
        None => println!("EDITOR_FRAME_RSS peak_kib=unavailable"),
    }

    Ok(())
}
