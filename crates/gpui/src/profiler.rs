use scheduler::Instant;
use std::{
    cell::LazyCell,
    collections::{HashMap, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::ThreadId,
};

#[cfg(feature = "frame-diagnostics")]
use std::{sync::atomic::AtomicU64, time::Duration};

use serde::{Deserialize, Serialize};

use crate::SharedString;
#[cfg(feature = "frame-diagnostics")]
use crate::{EntityId, WindowId};

#[doc(hidden)]
#[derive(Debug, Copy, Clone)]
pub struct TaskTiming {
    pub location: &'static core::panic::Location<'static>,
    pub start: Instant,
    pub end: Option<Instant>,
}

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct ThreadTaskTimings {
    pub thread_name: Option<String>,
    pub thread_id: ThreadId,
    pub timings: Vec<TaskTiming>,
    pub total_pushed: u64,
}

impl ThreadTaskTimings {
    /// Convert global thread timings into their structured format.
    pub fn convert(timings: &[GlobalThreadTimings]) -> Vec<Self> {
        timings
            .iter()
            .filter_map(|t| t.timings.upgrade().map(|timings| (t.thread_id, timings)))
            .map(|(thread_id, timings)| {
                let timings = timings.lock();
                let thread_name = timings.thread_name.clone();
                let total_pushed = timings.total_pushed;
                let timings = &timings.timings;

                let mut vec = Vec::with_capacity(timings.len());
                let (s1, s2) = timings.as_slices();
                vec.extend_from_slice(s1);
                vec.extend_from_slice(s2);

                ThreadTaskTimings {
                    thread_name,
                    thread_id,
                    timings: vec,
                    total_pushed,
                }
            })
            .collect()
    }
}

/// Serializable variant of [`core::panic::Location`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedLocation {
    /// Name of the source file
    pub file: SharedString,
    /// Line in the source file
    pub line: u32,
    /// Column in the source file
    pub column: u32,
}

impl From<&core::panic::Location<'static>> for SerializedLocation {
    fn from(value: &core::panic::Location<'static>) -> Self {
        SerializedLocation {
            file: value.file().into(),
            line: value.line(),
            column: value.column(),
        }
    }
}

/// Serializable variant of [`TaskTiming`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedTaskTiming {
    /// Location of the timing
    pub location: SerializedLocation,
    /// Time at which the measurement was reported in nanoseconds
    pub start: u128,
    /// Duration of the measurement in nanoseconds
    pub duration: u128,
}

impl SerializedTaskTiming {
    /// Convert an array of [`TaskTiming`] into their serializable format
    ///
    /// # Params
    ///
    /// `anchor` - [`Instant`] that should be earlier than all timings to use as base anchor
    pub fn convert(anchor: Instant, timings: &[TaskTiming]) -> Vec<SerializedTaskTiming> {
        let serialized = timings
            .iter()
            .map(|timing| {
                let start = timing.start.duration_since(anchor).as_nanos();
                let duration = timing
                    .end
                    .unwrap_or_else(|| Instant::now())
                    .duration_since(timing.start)
                    .as_nanos();
                SerializedTaskTiming {
                    location: timing.location.into(),
                    start,
                    duration,
                }
            })
            .collect::<Vec<_>>();

        serialized
    }
}

/// Serializable variant of [`ThreadTaskTimings`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedThreadTaskTimings {
    /// Thread name
    pub thread_name: Option<String>,
    /// Hash of the thread id
    pub thread_id: u64,
    /// Timing records for this thread
    pub timings: Vec<SerializedTaskTiming>,
}

impl SerializedThreadTaskTimings {
    /// Convert [`ThreadTaskTimings`] into their serializable format
    ///
    /// # Params
    ///
    /// `anchor` - [`Instant`] that should be earlier than all timings to use as base anchor
    pub fn convert(anchor: Instant, timings: ThreadTaskTimings) -> SerializedThreadTaskTimings {
        let serialized_timings = SerializedTaskTiming::convert(anchor, &timings.timings);

        let mut hasher = DefaultHasher::new();
        timings.thread_id.hash(&mut hasher);
        let thread_id = hasher.finish();

        SerializedThreadTaskTimings {
            thread_name: timings.thread_name,
            thread_id,
            timings: serialized_timings,
        }
    }
}

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct ThreadTimingsDelta {
    /// Hashed thread id
    pub thread_id: u64,
    /// Thread name, if known
    pub thread_name: Option<String>,
    /// New timings since the last call. If the circular buffer wrapped around
    /// since the previous poll, some entries may have been lost.
    pub new_timings: Vec<SerializedTaskTiming>,
}

/// Tracks which timing events have already been seen so that callers can request only unseen events.
#[doc(hidden)]
pub struct ProfilingCollector {
    startup_time: Instant,
    cursors: HashMap<ThreadId, u64>,
}

impl ProfilingCollector {
    pub fn new(startup_time: Instant) -> Self {
        Self {
            startup_time,
            cursors: HashMap::default(),
        }
    }

    pub fn startup_time(&self) -> Instant {
        self.startup_time
    }

    pub fn collect_unseen(
        &mut self,
        all_timings: Vec<ThreadTaskTimings>,
    ) -> Vec<ThreadTimingsDelta> {
        let mut deltas = Vec::with_capacity(all_timings.len());

        for thread in all_timings {
            let mut hasher = DefaultHasher::new();
            thread.thread_id.hash(&mut hasher);
            let hashed_id = hasher.finish();

            let prev_cursor = self.cursors.get(&thread.thread_id).copied().unwrap_or(0);
            let buffer_len = thread.timings.len() as u64;
            let buffer_start = thread.total_pushed.saturating_sub(buffer_len);

            let mut slice = if prev_cursor < buffer_start {
                // Cursor fell behind the buffer — some entries were evicted.
                // Return everything still in the buffer.
                thread.timings.as_slice()
            } else {
                let skip = (prev_cursor - buffer_start) as usize;
                &thread.timings[skip.min(thread.timings.len())..]
            };

            // Don't emit the last entry if it's still in-progress (end: None).
            let incomplete_at_end = slice.last().is_some_and(|t| t.end.is_none());
            if incomplete_at_end {
                slice = &slice[..slice.len() - 1];
            }

            let cursor_advance = if incomplete_at_end {
                thread.total_pushed.saturating_sub(1)
            } else {
                thread.total_pushed
            };

            self.cursors.insert(thread.thread_id, cursor_advance);

            if slice.is_empty() {
                continue;
            }

            let new_timings = SerializedTaskTiming::convert(self.startup_time, slice);

            deltas.push(ThreadTimingsDelta {
                thread_id: hashed_id,
                thread_name: thread.thread_name,
                new_timings,
            });
        }

        deltas
    }

    pub fn reset(&mut self) {
        self.cursors.clear();
    }
}

// Allow 16MiB of task timing entries.
// VecDeque grows by doubling its capacity when full, so keep this a power of 2 to avoid wasting
// memory.
const MAX_TASK_TIMINGS: usize = (16 * 1024 * 1024) / core::mem::size_of::<TaskTiming>();

#[doc(hidden)]
pub(crate) type TaskTimings = VecDeque<TaskTiming>;

#[doc(hidden)]
pub type GuardedTaskTimings = spin::Mutex<ThreadTimings>;

#[doc(hidden)]
pub struct GlobalThreadTimings {
    pub thread_id: ThreadId,
    pub timings: std::sync::Weak<GuardedTaskTimings>,
}

#[doc(hidden)]
pub static GLOBAL_THREAD_TIMINGS: spin::Mutex<Vec<GlobalThreadTimings>> =
    spin::Mutex::new(Vec::new());

thread_local! {
    #[doc(hidden)]
    pub static THREAD_TIMINGS: LazyCell<Arc<GuardedTaskTimings>> = LazyCell::new(|| {
        let current_thread = std::thread::current();
        let thread_name = current_thread.name();
        let thread_id = current_thread.id();
        let timings = ThreadTimings::new(thread_name.map(|e| e.to_owned()), thread_id);
        let timings = Arc::new(spin::Mutex::new(timings));

        {
            let timings = Arc::downgrade(&timings);
            let global_timings = GlobalThreadTimings {
                thread_id: std::thread::current().id(),
                timings,
            };
            GLOBAL_THREAD_TIMINGS.lock().push(global_timings);
        }

        timings
    });
}

#[doc(hidden)]
pub struct ThreadTimings {
    pub thread_name: Option<String>,
    pub thread_id: ThreadId,
    pub timings: TaskTimings,
    pub total_pushed: u64,
}

impl ThreadTimings {
    pub fn new(thread_name: Option<String>, thread_id: ThreadId) -> Self {
        ThreadTimings {
            thread_name,
            thread_id,
            timings: TaskTimings::new(),
            total_pushed: 0,
        }
    }

    /// If this task is the same as the last task, update the end time of the last task.
    ///
    /// Otherwise, add the new task timing to the list.
    pub fn add_task_timing(&mut self, timing: TaskTiming) {
        if let Some(last_timing) = self.timings.back_mut()
            && last_timing.location == timing.location
            && last_timing.start == timing.start
        {
            last_timing.end = timing.end;
        } else {
            while self.timings.len() + 1 > MAX_TASK_TIMINGS {
                // This should only ever pop one element because it matches the insertion below.
                self.timings.pop_front();
            }
            self.timings.push_back(timing);
            self.total_pushed += 1;
        }
    }

    pub fn get_thread_task_timings(&self) -> ThreadTaskTimings {
        ThreadTaskTimings {
            thread_name: self.thread_name.clone(),
            thread_id: self.thread_id,
            timings: self.timings.iter().cloned().collect(),
            total_pushed: self.total_pushed,
        }
    }
}

impl Drop for ThreadTimings {
    fn drop(&mut self) {
        let mut thread_timings = GLOBAL_THREAD_TIMINGS.lock();

        let Some((index, _)) = thread_timings
            .iter()
            .enumerate()
            .find(|(_, t)| t.thread_id == self.thread_id)
        else {
            return;
        };
        thread_timings.swap_remove(index);
    }
}

#[doc(hidden)]
pub fn add_task_timing(timing: TaskTiming) {
    if !PROFILER_ENABLED.load(Ordering::Acquire) {
        return;
    }
    THREAD_TIMINGS.with(|timings| {
        timings.lock().add_task_timing(timing);
    });
}

#[doc(hidden)]
pub fn get_current_thread_task_timings() -> ThreadTaskTimings {
    THREAD_TIMINGS.with(|timings| timings.lock().get_thread_task_timings())
}

static PROFILER_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables or disables task timing collection at runtime.
///
/// When transitioning from enabled to disabled, `add_task_timing` becomes a
/// no-op and the existing per-thread buffers are cleared so stale data isn't
/// reported after a later re-enable. Calls with the current value are a no-op.
pub fn set_enabled(enabled: bool) -> bool {
    if PROFILER_ENABLED.swap(enabled, Ordering::AcqRel) == enabled {
        return false;
    }

    if !enabled {
        for global in GLOBAL_THREAD_TIMINGS.lock().iter() {
            if let Some(timings) = global.timings.upgrade() {
                let mut timings = timings.lock();
                timings.timings.clear();
                timings.timings.shrink_to_fit();
                timings.total_pushed = 0;
            }
        }
    }
    true
}

/// Identifies one window frame build from its first invalidation through presentation.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FrameBuildId(u64);

#[cfg(feature = "frame-diagnostics")]
impl FrameBuildId {
    /// Returns the numeric identifier.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[cfg(feature = "frame-diagnostics")]
static NEXT_FRAME_BUILD_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "frame-diagnostics")]
pub(crate) fn next_frame_build_id() -> FrameBuildId {
    FrameBuildId(NEXT_FRAME_BUILD_ID.fetch_add(1, Ordering::Relaxed))
}

/// Describes why a window became dirty.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDirtyReason {
    /// The initial frame required when a window opens.
    Initial,
    /// An entity observed by the window emitted a notification.
    EntityNotify,
    /// The window requested a complete refresh.
    WindowRefresh,
}

/// Classifies the input event that caused an invalidation.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameInputProvenance {
    /// A key or modifier event.
    Keyboard,
    /// A mouse, scroll, gesture, or file-drag event.
    Pointer,
}

/// A measured portion of frame construction.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FramePhase {
    /// Element layout requests.
    RequestLayout,
    /// Hit testing, dispatch-tree construction, and other prepaint work.
    Prepaint,
    /// Scene and interaction painting.
    Paint,
    /// Reuse of cached prepaint state.
    PrepaintCacheReplay,
    /// Reuse of cached paint state and scene primitives.
    PaintCacheReplay,
}

/// One invalidation associated with a frame build.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub struct FrameInvalidation {
    /// The frame build receiving the invalidation.
    pub build_id: FrameBuildId,
    /// The affected window.
    pub window_id: WindowId,
    /// When the invalidation was observed.
    pub at: Instant,
    /// The notifying entity, when the invalidation came from `Context::notify`.
    pub entity_id: Option<EntityId>,
    /// Why the window became dirty.
    pub reason: FrameDirtyReason,
    /// The input class active when the invalidation occurred.
    pub input: Option<FrameInputProvenance>,
    /// Whether another invalidation had already created this frame build.
    pub coalesced: bool,
    /// Whether the invalidation occurred while drawing the current frame.
    pub during_draw: bool,
}

/// Aggregate timing for one frame phase.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub struct FramePhaseTiming {
    /// The measured frame build.
    pub build_id: FrameBuildId,
    /// The measured window.
    pub window_id: WindowId,
    /// The measured render phase.
    pub phase: FramePhase,
    /// Time spent in the phase.
    pub duration: Duration,
    /// Number of top-level operations represented by this sample.
    pub operations: u64,
    /// Number of cache replays represented by this sample.
    pub cache_hits: u64,
}

/// Timing for one completed window draw.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub struct FrameTiming {
    /// The completed frame build.
    pub build_id: FrameBuildId,
    /// The window that was drawn.
    pub window_id: WindowId,
    /// When the window first became dirty for this frame.
    pub dirty_at: Instant,
    /// Number of invalidations coalesced into the frame.
    pub invalidations: u64,
    /// When `Window::draw` began.
    pub draw_start: Instant,
    /// When `Window::draw` completed.
    pub draw_end: Instant,
    /// First input timestamp associated with the frame, when present.
    pub input_started_at: Option<Instant>,
    /// Input class associated with `input_started_at`.
    pub input: Option<FrameInputProvenance>,
}

#[cfg(feature = "frame-diagnostics")]
impl FrameTiming {
    /// Returns the time spent in `Window::draw`.
    pub fn draw_duration(&self) -> Duration {
        self.draw_end.duration_since(self.draw_start)
    }
}

/// Timing for submission of a newly built frame.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub struct FramePresentationTiming {
    /// The submitted frame build.
    pub build_id: FrameBuildId,
    /// The window whose frame was submitted.
    pub window_id: WindowId,
    /// When platform submission began.
    pub present_start: Instant,
    /// When platform submission completed.
    pub present_end: Instant,
    /// Time from the first associated input to completed submission.
    pub input_to_present: Option<Duration>,
    /// Input class associated with `input_to_present`.
    pub input: Option<FrameInputProvenance>,
}

/// A frame lifecycle event retained by the diagnostics journal.
#[cfg(feature = "frame-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub enum FrameEvent {
    /// A window or entity invalidated a frame.
    Invalidated(FrameInvalidation),
    /// A window began building a frame.
    DrawStarted {
        /// The frame build that began.
        build_id: FrameBuildId,
        /// The window being drawn.
        window_id: WindowId,
        /// When drawing began.
        at: Instant,
    },
    /// A render phase completed.
    Phase(FramePhaseTiming),
    /// A window completed drawing a frame.
    DrawFinished(FrameTiming),
    /// A newly drawn frame completed platform submission.
    Presented(FramePresentationTiming),
}

#[cfg(feature = "frame-diagnostics")]
const MAX_FRAME_EVENTS: usize = (16 * 1024 * 1024) / core::mem::size_of::<FrameEvent>();

#[cfg(feature = "frame-diagnostics")]
const INITIAL_FRAME_EVENTS_CAPACITY: usize = 4096;

#[cfg(feature = "frame-diagnostics")]
struct FrameEvents {
    events: VecDeque<FrameEvent>,
    total_pushed: u64,
}

#[cfg(feature = "frame-diagnostics")]
static FRAME_EVENTS: spin::Mutex<FrameEvents> = spin::Mutex::new(FrameEvents {
    events: VecDeque::new(),
    total_pushed: 0,
});

#[cfg(feature = "frame-diagnostics")]
pub(crate) fn record_frame_events(events: &[FrameEvent]) {
    if events.is_empty() {
        return;
    }
    let mut frame_events = FRAME_EVENTS.lock();
    if frame_events.events.capacity() < INITIAL_FRAME_EVENTS_CAPACITY {
        let additional_capacity =
            INITIAL_FRAME_EVENTS_CAPACITY.saturating_sub(frame_events.events.capacity());
        frame_events.events.reserve(additional_capacity);
    }
    for event in events {
        if frame_events.events.len() >= MAX_FRAME_EVENTS {
            frame_events.events.pop_front();
        }
        frame_events.events.push_back(*event);
        frame_events.total_pushed += 1;
    }
}

/// Frame events collected since the previous snapshot.
#[cfg(feature = "frame-diagnostics")]
#[derive(Debug)]
pub struct FrameDiagnosticsSnapshot {
    /// Events recorded since the previous snapshot.
    pub events: Vec<FrameEvent>,
    /// Events evicted before this collector could observe them.
    pub dropped_events: u64,
}

/// Collects frame events without removing them from other collectors.
#[cfg(feature = "frame-diagnostics")]
pub struct FrameTimingCollector {
    cursor: u64,
}

#[cfg(feature = "frame-diagnostics")]
impl Default for FrameTimingCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "frame-diagnostics")]
impl FrameTimingCollector {
    /// Starts observing events recorded after this call.
    pub fn new() -> Self {
        Self {
            cursor: FRAME_EVENTS.lock().total_pushed,
        }
    }

    /// Returns events recorded since the previous snapshot.
    pub fn snapshot(&mut self) -> FrameDiagnosticsSnapshot {
        let frame_events = FRAME_EVENTS.lock();
        let buffer_len = frame_events.events.len() as u64;
        let buffer_start = frame_events.total_pushed.saturating_sub(buffer_len);
        let dropped_events = buffer_start.saturating_sub(self.cursor);
        let skip = self.cursor.saturating_sub(buffer_start) as usize;
        let events = frame_events
            .events
            .iter()
            .skip(skip.min(frame_events.events.len()))
            .copied()
            .collect();
        self.cursor = frame_events.total_pushed;
        FrameDiagnosticsSnapshot {
            events,
            dropped_events,
        }
    }
}
