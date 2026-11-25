//! Hardware, resource, and allocation counters for ZZZ perf workloads.
//!
//! ZZZ's perf harness ([`perf`](https://docs.rs/perf)) times `#[perf]` tests with
//! hyperfine and reads numeric `TAG key=value` fields out of a workload's
//! stdout for soak and regression tracking. This crate supplies the counters a
//! workload can fill those fields with: retired instructions and CPU cycles
//! from the kernel's performance-monitoring unit, `getrusage` statistics that
//! need no privileges, and heap-allocation totals.
//!
//! Each counter either opens its backing resource in [`new`](HardwareCounter::new)
//! or, for allocations, checks that [`CountingAllocator`] is the binary's global
//! allocator. A counter that cannot work on this machine fails at construction
//! with an actionable error, so a workload can print `note=unavailable` instead
//! of measuring zeros.
//!
//! # Wiring a workload
//!
//! Print the counter's `key=value` field on the workload's own metric line, as
//! `editor_frame_workload` does:
//!
//! ```ignore
//! let instructions = bench_metrics::HardwareCounter::new(
//!     bench_metrics::HardwareEvent::Instructions,
//!     bench_metrics::MetricScope::CallingThread,
//! )?;
//! let start = instructions.start();
//! run_the_workload();
//! println!("WORKLOAD_TAG instructions={}", instructions.finish(start));
//! ```
//!
//! The perf tool's `parse_metric_line` folds each numeric field into
//! `<tag>.<key>`, so `cargo perf-soak` reports the counter's slope alongside
//! wall time and `cargo perf-compare` gates on it across runs.

use anyhow::{Result, anyhow};

/// Which threads a counter covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricScope {
    /// Every thread in the benchmark process, including threads created after
    /// the counter was opened.
    Process,
    /// Only the thread that constructed the counter. In a workload that is the
    /// thread calling `start` and `finish`.
    CallingThread,
}

impl MetricScope {
    const fn label(self) -> &'static str {
        match self {
            Self::Process => "the benchmark process",
            Self::CallingThread => "the calling thread",
        }
    }
}

/// One counter's reading at an instant, returned by [`HardwareCounter::start`],
/// [`ResourceCounter::start`], and [`AllocationCounter::start`]. Opaque so a
/// backend can choose its own representation.
pub struct Sample<T>(T);

/// A CPU event counted by [`HardwareCounter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareEvent {
    /// Retired userspace instructions: near-deterministic, so small
    /// regressions are detectable, but blind to waits and memory stalls.
    Instructions,
    /// Userspace CPU cycles. `instructions / cycles` is IPC; a falling IPC with
    /// flat instructions means worse cache or branch behavior.
    Cycles,
    /// Mispredicted branches. Linux only.
    BranchMisses,
    /// Last-level cache misses, i.e. accesses served from memory. Linux only.
    CacheMisses,
    /// Last-level cache accesses, the denominator for [`CacheMisses`](Self::CacheMisses).
    /// Linux only.
    CacheReferences,
}

impl HardwareEvent {
    /// The event name the counter reports.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Instructions => "instructions",
            Self::Cycles => "cycles",
            Self::BranchMisses => "branch-misses",
            Self::CacheMisses => "cache-misses",
            Self::CacheReferences => "cache-references",
        }
    }

    /// The noun a report prints after the count.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Instructions => "instructions",
            Self::Cycles => "cycles",
            Self::BranchMisses => "branch misses",
            Self::CacheMisses => "cache misses",
            Self::CacheReferences => "cache references",
        }
    }
}

/// A hardware performance counter over a set of threads.
///
/// Opening a counter needs kernel support: on Linux `perf_event_open` must be
/// permitted (CAP_PERFMON, or `perf_event_paranoid <= 2`), and on Apple Silicon
/// the XNU interfaces must expose the event. A machine that refuses is reported
/// by [`new`](Self::new) rather than by counting zeros.
pub struct HardwareCounter {
    event: HardwareEvent,
    scope: MetricScope,
    backend: backend::Counter,
}

impl HardwareCounter {
    /// Opens a counter for `event` over `scope`.
    pub fn new(event: HardwareEvent, scope: MetricScope) -> Result<Self> {
        let backend = backend::Counter::open(event, scope)?;
        let this = Self {
            event,
            scope,
            backend,
        };
        this.backend.verify_advances().map_err(|error| {
            anyhow!(
                "{error}; hardware performance counters are unavailable to this machine or CI \
                     runner"
            )
        })?;
        Ok(this)
    }

    /// The counted event.
    pub const fn event(&self) -> HardwareEvent {
        self.event
    }

    /// The covered threads.
    pub const fn scope(&self) -> MetricScope {
        self.scope
    }

    /// Reads the counter so [`finish`](Self::finish) can report the delta.
    pub fn start(&self) -> Sample<backend::Snapshot> {
        Sample(self.backend.snapshot())
    }

    /// The count between `start` and now, in the event's unit.
    ///
    /// # Panics
    ///
    /// Panics when the counter did not run during the interval, which means the
    /// machine stopped scheduling the event after it verified at construction.
    pub fn finish(&self, start: Sample<backend::Snapshot>) -> f64 {
        self.backend.delta(&start.0, &self.backend.snapshot())
    }
}

#[cfg(target_os = "macos")]
use darwin as backend;
#[cfg(target_os = "linux")]
use perf as backend;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use unsupported as backend;

/// Linux backend: `perf_event_open` through the `perf-event2` crate, whose
/// items are referenced through the `perf_event` path so this module's own
/// helpers stay distinguishable from the crate's.
///
/// With [`MetricScope::Process`], one inherited counter is opened for every
/// thread that exists at construction. Inheritance extends each counter to the
/// threads that thread later creates, and live descendants are summed on read,
/// so the whole process is covered for the counter's lifetime. With
/// [`MetricScope::CallingThread`], a single non-inherited counter observes the
/// constructing thread only.
///
/// Hybrid CPUs (separate performance and efficiency core PMUs) need one event
/// per PMU, of which only the one matching the CPU a thread runs on can be
/// scheduled; each PMU's encoding is read from sysfs. If the kernel multiplexes
/// a conventional counter with other profiling, its count is scaled by
/// `time_enabled / time_running` and a warning is printed once.
#[cfg(target_os = "linux")]
pub mod perf {
    use super::{HardwareEvent, MetricScope};
    use anyhow::{Context as _, Result, anyhow};
    use std::cell::{Cell, RefCell};

    pub(super) struct Counter {
        event: HardwareEvent,
        counters: RefCell<Vec<PerfCounter>>,
        multiplexing_reported: Cell<bool>,
    }

    /// Every counter's state at one instant.
    #[doc(hidden)]
    pub struct Snapshot(Vec<CounterState>);

    struct PerfCounter {
        counter: perf_event::Counter,
        /// Generic events on conventional CPUs are scaled when multiplexed.
        /// Hybrid per-PMU events are not, since a thread on the other core type
        /// shows as "not running" for that PMU by design.
        scale_for_multiplexing: bool,
    }

    #[derive(Clone, Copy)]
    struct CounterState {
        count: u64,
        time_enabled: std::time::Duration,
        time_running: std::time::Duration,
    }

    impl Counter {
        pub(super) fn open(event: HardwareEvent, scope: MetricScope) -> Result<Self> {
            let hybrid_events = hybrid_pmu_events(event)?;
            let mut counters = Vec::new();
            match scope {
                MetricScope::CallingThread => {
                    Self::open_target(
                        event,
                        CounterTarget::CallingThread,
                        &hybrid_events,
                        &mut counters,
                    )?;
                }
                MetricScope::Process => {
                    for thread_id in process_thread_ids()? {
                        Self::open_target(
                            event,
                            CounterTarget::ThreadAndDescendants(thread_id),
                            &hybrid_events,
                            &mut counters,
                        )?;
                    }
                }
            }
            if counters.is_empty() {
                return Err(anyhow!(
                    "no Linux {} counter could be opened for {}: the CPU does not expose this event",
                    event.name(),
                    scope.label()
                ));
            }
            for counter in &mut counters {
                counter
                    .counter
                    .enable()
                    .map_err(|error| Self::operation_error(event, "enable", error))?;
            }
            Ok(Self {
                event,
                counters: RefCell::new(counters),
                multiplexing_reported: Cell::new(false),
            })
        }

        fn open_target(
            event: HardwareEvent,
            target: CounterTarget,
            hybrid_events: &[(u32, u64)],
            counters: &mut Vec<PerfCounter>,
        ) -> Result<()> {
            let opened: Vec<(std::io::Result<perf_event::Counter>, bool)> =
                if hybrid_events.is_empty() {
                    let mut builder = perf_event::Builder::new(generic_event(event));
                    target.configure(&mut builder);
                    vec![(builder.build(), true)]
                } else {
                    hybrid_events
                        .iter()
                        .map(|&(pmu_type, config)| {
                            let mut builder =
                                perf_event::Builder::new(perf_event::events::Raw::new(config));
                            builder.attrs_mut().type_ = pmu_type;
                            target.configure(&mut builder);
                            (builder.build(), false)
                        })
                        .collect()
                };
            for (result, scale_for_multiplexing) in opened {
                match result {
                    Ok(counter) => counters.push(PerfCounter {
                        counter,
                        scale_for_multiplexing,
                    }),
                    // The thread exited between enumeration and open.
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(Self::open_error(event, target, error)),
                }
            }
            Ok(())
        }

        fn open_error(
            event: HardwareEvent,
            target: CounterTarget,
            error: std::io::Error,
        ) -> anyhow::Error {
            anyhow!(
                "failed to open Linux {} counter for {target}: {error}. Grant this workload \
                 CAP_PERFMON or adjust /proc/sys/kernel/perf_event_paranoid according to your CI \
                 security policy",
                event.name()
            )
        }

        fn operation_error(
            event: HardwareEvent,
            operation: &str,
            error: std::io::Error,
        ) -> anyhow::Error {
            anyhow!("Linux {} counter {operation} failed: {error}", event.name())
        }

        pub(super) fn verify_advances(&self) -> Result<()> {
            let before = self.snapshot();
            let mut spin = 0_u64;
            for index in 0..50_000_u64 {
                spin = spin.wrapping_add(index.rotate_left(7));
            }
            std::hint::black_box(spin);
            let after = self.snapshot();
            if self.advanced(&before, &after) {
                Ok(())
            } else {
                Err(anyhow!(
                    "the {} counter opened but does not advance",
                    self.event.name()
                ))
            }
        }

        pub(super) fn snapshot(&self) -> Snapshot {
            Snapshot(
                self.counters
                    .borrow_mut()
                    .iter_mut()
                    .map(|counter| {
                        let data = counter.counter.read_full().unwrap_or_else(|error| {
                            panic!("{}", Self::operation_error(self.event, "read", error))
                        });
                        CounterState {
                            count: data.count(),
                            time_enabled: data
                                .time_enabled()
                                .expect("time-enabled counter data was requested"),
                            time_running: data
                                .time_running()
                                .expect("time-running counter data was requested"),
                        }
                    })
                    .collect(),
            )
        }

        fn advanced(&self, before: &Snapshot, after: &Snapshot) -> bool {
            before
                .0
                .iter()
                .zip(&after.0)
                .any(|(before, after)| after.time_running > before.time_running)
        }

        pub(super) fn delta(&self, start: &Snapshot, end: &Snapshot) -> f64 {
            let counters = self.counters.borrow();
            let mut total = 0.0;
            let mut any_counter_ran = false;
            for ((counter, start), end) in counters.iter().zip(&start.0).zip(&end.0) {
                let count = end.count.saturating_sub(start.count) as f64;
                let time_enabled = end.time_enabled.saturating_sub(start.time_enabled);
                let time_running = end.time_running.saturating_sub(start.time_running);
                if time_running.is_zero() {
                    // Process-wide coverage includes threads that slept for the
                    // whole interval, and hybrid PMUs whose core type this thread
                    // never ran on.
                    continue;
                }
                any_counter_ran = true;
                if counter.scale_for_multiplexing && time_running < time_enabled {
                    if !self.multiplexing_reported.replace(true) {
                        eprintln!(
                            "{} counters were multiplexed by the kernel; counts are scaled using \
                             time_enabled/time_running",
                            self.event.name()
                        );
                    }
                    total += count * time_enabled.as_secs_f64() / time_running.as_secs_f64();
                } else {
                    total += count;
                }
            }
            assert!(
                any_counter_ran,
                "Linux {} counters did not run during the measured interval; ensure hardware \
                 performance counters are available to this machine or CI runner",
                self.event.name()
            );
            total
        }
    }

    fn generic_event(event: HardwareEvent) -> perf_event::events::Hardware {
        use perf_event::events::Hardware;
        match event {
            HardwareEvent::Instructions => Hardware::INSTRUCTIONS,
            HardwareEvent::Cycles => Hardware::CPU_CYCLES,
            HardwareEvent::BranchMisses => Hardware::BRANCH_MISSES,
            HardwareEvent::CacheMisses => Hardware::CACHE_MISSES,
            HardwareEvent::CacheReferences => Hardware::CACHE_REFERENCES,
        }
    }

    #[derive(Clone, Copy)]
    enum CounterTarget {
        /// `pid = 0` to `perf_event_open`: the calling thread, with no inheritance.
        CallingThread,
        /// A specific thread plus every thread it creates afterwards.
        ThreadAndDescendants(i32),
    }

    impl CounterTarget {
        fn configure(self, builder: &mut perf_event::Builder) {
            use perf_event::ReadFormat;

            match self {
                Self::CallingThread => builder.observe_self().inherit(false),
                Self::ThreadAndDescendants(thread_id) => {
                    builder.observe_pid(thread_id).inherit(true)
                }
            };
            builder.read_format(ReadFormat::TOTAL_TIME_ENABLED | ReadFormat::TOTAL_TIME_RUNNING);
        }
    }

    impl std::fmt::Display for CounterTarget {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::CallingThread => write!(f, "the calling thread"),
                Self::ThreadAndDescendants(thread_id) => write!(f, "thread {thread_id}"),
            }
        }
    }

    fn process_thread_ids() -> Result<Vec<i32>> {
        let mut thread_ids = Vec::new();
        for entry in std::fs::read_dir("/proc/self/task")
            .map_err(|error| anyhow!("failed to enumerate workload process threads: {error}"))?
        {
            let entry = entry.map_err(|error| {
                anyhow!("failed to enumerate a workload process thread: {error}")
            })?;
            if let Some(thread_id) = entry
                .file_name()
                .to_str()
                .and_then(|thread_id| thread_id.parse().ok())
            {
                thread_ids.push(thread_id);
            }
        }
        Ok(thread_ids)
    }

    /// Returns `(pmu_type, config)` for `event` on each hybrid CPU PMU, or an
    /// empty list on conventional CPUs where the generic hardware event suffices.
    ///
    /// A PMU (performance monitoring unit) is the per-core block of counter
    /// registers; Linux exposes each as a device under
    /// `/sys/bus/event_source/devices/`. Conventional CPUs have one, `cpu`, and
    /// the generic events target it. Hybrid CPUs have `cpu_core` (performance
    /// cores) and `cpu_atom` (efficiency cores) with distinct event encodings,
    /// and the kernel refuses the generic event for a task that may run on
    /// either, so one raw event per PMU is needed.
    fn hybrid_pmu_events(event: HardwareEvent) -> Result<Vec<(u32, u64)>> {
        let mut events = Vec::new();
        for pmu in ["cpu_core", "cpu_atom"] {
            let pmu_path = std::path::Path::new("/sys/bus/event_source/devices").join(pmu);
            if !pmu_path.exists() {
                continue;
            }
            let pmu_type = std::fs::read_to_string(pmu_path.join("type"))
                .with_context(|| format!("failed to read Linux {pmu} performance-counter type"))?
                .trim()
                .parse::<u32>()
                .with_context(|| format!("invalid Linux {pmu} performance-counter type"))?;
            let encoding = std::fs::read_to_string(pmu_path.join("events").join(event.name()))
                .with_context(|| format!("failed to read Linux {pmu} {} event", event.name()))?;
            let mut config = 0_u64;
            for term in encoding.trim().split(',') {
                let (name, value) = term.split_once('=').unwrap_or((term, "1"));
                let value = value
                    .strip_prefix("0x")
                    .map_or_else(|| value.parse::<u64>(), |hex| u64::from_str_radix(hex, 16))
                    .with_context(|| {
                        format!("invalid Linux {pmu} {} event term {term:?}", event.name())
                    })?;
                let layout = std::fs::read_to_string(pmu_path.join("format").join(name))
                    .with_context(|| format!("failed to read Linux {pmu} format for {name:?}"))?;
                let bits = layout.trim().strip_prefix("config:").ok_or_else(|| {
                    anyhow!(
                        "unsupported Linux {pmu} format {:?} for {name:?}",
                        layout.trim()
                    )
                })?;
                let (low, high) = if let Some((low, high)) = bits.split_once('-') {
                    (low.parse::<u32>()?, high.parse::<u32>()?)
                } else {
                    let bit = bits.parse::<u32>()?;
                    (bit, bit)
                };
                let width = high - low + 1;
                let mask = if width >= 64 {
                    u64::MAX
                } else {
                    (1_u64 << width) - 1
                };
                config |= (value & mask) << low;
            }
            events.push((pmu_type, config));
        }
        Ok(events)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn perf_permission_error_is_actionable() {
            let error = Counter::open_error(
                HardwareEvent::Instructions,
                CounterTarget::ThreadAndDescendants(42),
                std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            );
            let message = error.to_string();
            assert!(message.contains("CAP_PERFMON"));
            assert!(message.contains("perf_event_paranoid"));
            assert!(message.contains("thread 42"));
        }
    }
}

/// macOS backend. Apple Silicon exposes retired instructions and cycles to
/// unprivileged processes through two XNU interfaces: `proc_pid_rusage` with
/// `RUSAGE_INFO_V4` for the whole process (every thread, including exited
/// ones), and `thread_selfcounts` for the calling thread. Both are plain
/// reads with no counter to open or close. Intel Macs report zero for both,
/// which [`Counter::verify_advances`] detects. Branch and cache events require
/// the private `kperf` framework and root, so they are unsupported here.
#[cfg(target_os = "macos")]
pub mod darwin {
    use super::{HardwareEvent, MetricScope};
    use anyhow::{Result, anyhow};

    pub(super) struct Counter {
        event: HardwareEvent,
        scope: MetricScope,
    }

    /// `[instructions, cycles]` at one instant.
    #[doc(hidden)]
    pub struct Snapshot([u64; 2]);

    /// The leading fields of `struct rusage_info_v4` from `<sys/resource.h>`,
    /// up to and including the two counters this backend reads. Only a
    /// prefix is declared: the kernel copies out `sizeof` the flavor's full
    /// struct, so the buffer below is padded to that size.
    #[repr(C)]
    struct RusageInfoV4Prefix {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
        ri_child_user_time: u64,
        ri_child_system_time: u64,
        ri_child_pkg_idle_wkups: u64,
        ri_child_interrupt_wkups: u64,
        ri_child_pageins: u64,
        ri_child_elapsed_abstime: u64,
        ri_diskio_bytesread: u64,
        ri_diskio_byteswritten: u64,
        ri_cpu_time_qos_default: u64,
        ri_cpu_time_qos_maintenance: u64,
        ri_cpu_time_qos_background: u64,
        ri_cpu_time_qos_utility: u64,
        ri_cpu_time_qos_legacy: u64,
        ri_cpu_time_qos_user_initiated: u64,
        ri_cpu_time_qos_user_interactive: u64,
        ri_billed_system_time: u64,
        ri_serviced_system_time: u64,
        ri_logical_writes: u64,
        ri_lifetime_max_phys_footprint: u64,
        ri_instructions: u64,
        ri_cycles: u64,
    }

    /// `sizeof(struct rusage_info_v4)`: the prefix above plus the four trailing
    /// `u64` fields (`ri_billed_energy`, `ri_serviced_energy`,
    /// `ri_interval_max_phys_footprint`, `ri_runnable_time`).
    const RUSAGE_INFO_V4_SIZE: usize = std::mem::size_of::<RusageInfoV4Prefix>() + 4 * 8;
    const RUSAGE_INFO_V4: libc::c_int = 4;
    /// `THSC_CPI` in `<sys/thread_selfcounts.h>` (not in `libc`).
    const THREAD_SELFCOUNTS_CPI: libc::c_int = 1;

    #[repr(C, align(8))]
    struct RusageInfoV4Buffer([u8; RUSAGE_INFO_V4_SIZE]);

    unsafe extern "C" {
        fn proc_pid_rusage(
            pid: libc::c_int,
            flavor: libc::c_int,
            buffer: *mut RusageInfoV4Buffer,
        ) -> libc::c_int;
        fn thread_selfcounts(
            kind: libc::c_int,
            buffer: *mut u64,
            size: libc::size_t,
        ) -> libc::c_int;
    }

    impl Counter {
        pub(super) fn open(event: HardwareEvent, scope: MetricScope) -> Result<Self> {
            if !matches!(event, HardwareEvent::Instructions | HardwareEvent::Cycles) {
                return Err(anyhow!(
                    "macOS exposes only instructions and cycles without root; {} needs kperf",
                    event.name()
                ));
            }
            let this = Self { event, scope };
            this.read().map_err(|error| {
                anyhow!(
                    "{error}; hardware counters are unavailable for {} on this Mac (Intel Macs and \
                     macOS virtual machines such as CI runners do not expose them)",
                    scope.label()
                )
            })?;
            Ok(this)
        }

        pub(super) fn verify_advances(&self) -> Result<()> {
            let before = self.snapshot();
            let mut spin = 0_u64;
            for index in 0..50_000_u64 {
                spin = spin.wrapping_add(index.rotate_left(7));
            }
            std::hint::black_box(spin);
            let after = self.snapshot();
            if self.advanced(&before, &after) {
                Ok(())
            } else {
                Err(anyhow!(
                    "the {} counter for {} does not advance",
                    self.event.name(),
                    self.scope.label()
                ))
            }
        }

        fn read(&self) -> Result<[u64; 2]> {
            match self.scope {
                MetricScope::Process => {
                    let mut buffer = RusageInfoV4Buffer([0; RUSAGE_INFO_V4_SIZE]);
                    // SAFETY: the buffer is the flavor's full size and aligned
                    // for its `u64` fields; the kernel fills it on success.
                    let status = unsafe {
                        proc_pid_rusage(
                            std::process::id() as libc::c_int,
                            RUSAGE_INFO_V4,
                            &mut buffer,
                        )
                    };
                    if status != 0 {
                        return Err(anyhow!(
                            "proc_pid_rusage failed: {}",
                            std::io::Error::last_os_error()
                        ));
                    }
                    // SAFETY: the buffer is at least as large and as aligned as
                    // the prefix struct, and every field is a plain integer.
                    let info = unsafe { &*(buffer.0.as_ptr() as *const RusageInfoV4Prefix) };
                    Ok([info.ri_instructions, info.ri_cycles])
                }
                MetricScope::CallingThread => {
                    let mut counts = [0_u64; 2];
                    // SAFETY: `counts` holds the two `u64`s the CPI flavor writes.
                    let status = unsafe {
                        thread_selfcounts(
                            THREAD_SELFCOUNTS_CPI,
                            counts.as_mut_ptr(),
                            std::mem::size_of_val(&counts),
                        )
                    };
                    if status != 0 {
                        return Err(anyhow!(
                            "thread_selfcounts failed: {}",
                            std::io::Error::last_os_error()
                        ));
                    }
                    Ok(counts)
                }
            }
        }

        const fn value(&self, snapshot: &Snapshot) -> u64 {
            match self.event {
                HardwareEvent::Instructions => snapshot.0[0],
                _ => snapshot.0[1],
            }
        }

        pub(super) fn snapshot(&self) -> Snapshot {
            Snapshot(self.read().unwrap_or_else(|error| panic!("{error}")))
        }

        fn advanced(&self, before: &Snapshot, after: &Snapshot) -> bool {
            self.value(after) > self.value(before)
        }

        pub(super) fn delta(&self, start: &Snapshot, end: &Snapshot) -> f64 {
            self.value(end).saturating_sub(self.value(start)) as f64
        }
    }
}

/// Fallback backend for platforms without a hardware-counter source.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub mod unsupported {
    use super::{HardwareEvent, MetricScope};
    use anyhow::{Result, anyhow};

    pub(super) struct Counter;

    #[doc(hidden)]
    pub struct Snapshot;

    impl Counter {
        pub(super) fn open(event: HardwareEvent, scope: MetricScope) -> Result<Self> {
            Err(anyhow!(
                "hardware counters ({} for {}) are unavailable on this platform",
                event.name(),
                scope.label()
            ))
        }

        pub(super) fn verify_advances(&self) -> Result<()> {
            Ok(())
        }

        pub(super) fn snapshot(&self) -> Snapshot {
            Snapshot
        }

        pub(super) fn delta(&self, _: &Snapshot, _: &Snapshot) -> f64 {
            0.0
        }
    }
}

/// A statistic reported by `getrusage`, counted by [`ResourceCounter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceMetric {
    /// Times the thread blocked and gave up the CPU: lock waits, channel
    /// receives, sleeps, and I/O. Preemptions are not included, since they
    /// reflect the machine rather than the code.
    VoluntaryContextSwitches,
    /// Page faults served without disk I/O: first touches of freshly mapped
    /// memory, a privilege-free proxy for allocation pressure.
    MinorPageFaults,
}

impl ResourceMetric {
    /// The unit noun used in reports.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::VoluntaryContextSwitches => "context switches",
            Self::MinorPageFaults => "page faults",
        }
    }

    #[cfg(unix)]
    fn read(self, usage: &libc::rusage) -> u64 {
        let value = match self {
            Self::VoluntaryContextSwitches => usage.ru_nvcsw,
            Self::MinorPageFaults => usage.ru_minflt,
        };
        value.max(0) as u64
    }
}

/// A `getrusage` statistic over a set of threads.
///
/// Unlike hardware counters this needs no privileges and works on every Unix.
/// [`MetricScope::CallingThread`] uses `RUSAGE_THREAD`, which only Linux
/// provides; [`MetricScope::Process`] uses `RUSAGE_SELF`, which sums every
/// thread in the process.
pub struct ResourceCounter {
    metric: ResourceMetric,
    scope: MetricScope,
}

impl ResourceCounter {
    /// Creates a counter for `metric` over `scope`.
    pub fn new(metric: ResourceMetric, scope: MetricScope) -> Result<Self> {
        let this = Self { metric, scope };
        this.read()?;
        Ok(this)
    }

    /// The counted statistic.
    pub const fn metric(&self) -> ResourceMetric {
        self.metric
    }

    /// The covered threads.
    pub const fn scope(&self) -> MetricScope {
        self.scope
    }

    /// Reads the statistic so [`finish`](Self::finish) can report the delta.
    pub fn start(&self) -> Sample<u64> {
        Sample(self.read().unwrap_or_else(|error| panic!("{error}")))
    }

    /// The count between `start` and now, in the metric's unit.
    pub fn finish(&self, start: Sample<u64>) -> f64 {
        let end = self.read().unwrap_or_else(|error| panic!("{error}"));
        end.saturating_sub(start.0) as f64
    }

    #[cfg(unix)]
    fn read(&self) -> Result<u64> {
        let who = match self.scope {
            MetricScope::Process => libc::RUSAGE_SELF,
            #[cfg(target_os = "linux")]
            MetricScope::CallingThread => libc::RUSAGE_THREAD,
            #[cfg(not(target_os = "linux"))]
            MetricScope::CallingThread => {
                return Err(anyhow!(
                    "getrusage statistics for {} need RUSAGE_THREAD, which only Linux provides",
                    self.scope.label()
                ));
            }
        };
        // SAFETY: `rusage` is plain old data that `getrusage` fully initializes
        // on success, and `who` is one of the constants the call accepts.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        let status = unsafe { libc::getrusage(who, &raw mut usage) };
        if status != 0 {
            return Err(anyhow!(
                "getrusage failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(self.metric.read(&usage))
    }

    #[cfg(not(unix))]
    fn read(&self) -> Result<u64> {
        Err(anyhow!(
            "getrusage statistics for {} are unavailable on this platform",
            self.scope.label()
        ))
    }
}

// ---------------------------------------------------------------------------
// Allocation counting
// ---------------------------------------------------------------------------

/// A global allocator wrapper that counts every allocation and free.
///
/// Install it as the workload binary's global allocator, then read
/// [`allocation_stats`] from the workload. Counters are cache-line sharded so
/// threads allocating concurrently rarely contend.
pub struct CountingAllocator<A = std::alloc::System> {
    inner: A,
}

impl<A> CountingAllocator<A> {
    /// Wraps `inner`, which serves every allocation.
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

// SAFETY: every method forwards to `inner` with the caller's arguments and
// returns its result unchanged; counting never touches the memory.
unsafe impl<A: std::alloc::GlobalAlloc> std::alloc::GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        let pointer = unsafe { self.inner.alloc(layout) };
        if !pointer.is_null() {
            allocation_counts::record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        let pointer = unsafe { self.inner.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocation_counts::record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: the pointer and layout pair came from this allocator.
        unsafe { self.inner.dealloc(pointer, layout) };
        allocation_counts::record_free(layout.size());
    }

    unsafe fn realloc(
        &self,
        pointer: *mut u8,
        layout: std::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        // SAFETY: the pointer and layout pair came from this allocator.
        let new_pointer = unsafe { self.inner.realloc(pointer, layout, new_size) };
        // A reallocation is counted as a new allocation replacing the old one,
        // as dhat and heaptrack count it, so growing a `Vec` shows up as churn.
        if !new_pointer.is_null() {
            allocation_counts::record_free(layout.size());
            allocation_counts::record_allocation(new_size);
        }
        new_pointer
    }
}

/// Totals counted by [`CountingAllocator`] since the process started.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationStats {
    /// Allocations served, including reallocations.
    pub allocations: u64,
    /// Bytes requested by those allocations.
    pub allocated_bytes: u64,
    /// Bytes returned by frees and by the old side of reallocations.
    pub freed_bytes: u64,
}

impl AllocationStats {
    /// Bytes allocated and not yet freed.
    pub const fn live_bytes(&self) -> i64 {
        self.allocated_bytes as i64 - self.freed_bytes as i64
    }
}

/// Returns the process's allocation totals, or `None` when this binary's
/// global allocator is not a [`CountingAllocator`]. Differences between two
/// calls give the allocations, bytes, and live-heap change of the code between
/// them, across every thread.
pub fn allocation_stats() -> Option<AllocationStats> {
    allocation_counts::installed().then(allocation_counts::read)
}

/// Counters shared by every thread, plus each thread's own. Each thread
/// increments one of several cache-line-aligned shards, so threads allocating
/// concurrently rarely contend on a line; readers sum the shards. The
/// per-thread counts are plain cells only their own thread reads.
mod allocation_counts {
    use super::AllocationStats;
    use std::{
        cell::Cell,
        sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    };

    const SHARD_COUNT: usize = 32;

    #[repr(align(128))]
    struct Shard {
        allocations: AtomicU64,
        allocated_bytes: AtomicU64,
        freed_bytes: AtomicU64,
    }

    static SHARDS: [Shard; SHARD_COUNT] = [const {
        Shard {
            allocations: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }; SHARD_COUNT];
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    static NEXT_SHARD: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        // `const` and without `Drop`, so accessing these never allocates and
        // still works while the thread's other thread-locals are being destroyed.
        static SHARD_INDEX: Cell<Option<usize>> = const { Cell::new(None) };
        static THREAD_ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
        static THREAD_ALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
    }

    fn shard() -> &'static Shard {
        let index = SHARD_INDEX
            .try_with(|index| {
                if let Some(index) = index.get() {
                    index
                } else {
                    let next = NEXT_SHARD.fetch_add(1, Ordering::Relaxed) % SHARD_COUNT;
                    index.set(Some(next));
                    next
                }
            })
            .unwrap_or(0);
        &SHARDS[index % SHARD_COUNT]
    }

    pub(super) fn record_allocation(size: usize) {
        // Loaded first so the flag's cache line stays shared across threads
        // instead of being written on every allocation.
        if !INSTALLED.load(Ordering::Relaxed) {
            INSTALLED.store(true, Ordering::Relaxed);
        }
        let shard = shard();
        shard.allocations.fetch_add(1, Ordering::Relaxed);
        shard
            .allocated_bytes
            .fetch_add(size as u64, Ordering::Relaxed);
        // Fails only while the thread is exiting, when nothing reads its counts.
        THREAD_ALLOCATIONS
            .try_with(|count| count.set(count.get() + 1))
            .ok();
        THREAD_ALLOCATED_BYTES
            .try_with(|bytes| bytes.set(bytes.get() + size as u64))
            .ok();
    }

    pub(super) fn record_free(size: usize) {
        shard()
            .freed_bytes
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    pub(super) fn installed() -> bool {
        INSTALLED.load(Ordering::Relaxed)
    }

    /// The calling thread's allocations and allocated bytes; frees are not
    /// tracked per thread, since a block is often freed by another thread.
    pub(super) fn read_calling_thread() -> AllocationStats {
        AllocationStats {
            allocations: THREAD_ALLOCATIONS.try_with(Cell::get).unwrap_or(0),
            allocated_bytes: THREAD_ALLOCATED_BYTES.try_with(Cell::get).unwrap_or(0),
            freed_bytes: 0,
        }
    }

    pub(super) fn read() -> AllocationStats {
        SHARDS
            .iter()
            .fold(AllocationStats::default(), |total, shard| AllocationStats {
                allocations: total.allocations + shard.allocations.load(Ordering::Relaxed),
                allocated_bytes: total.allocated_bytes
                    + shard.allocated_bytes.load(Ordering::Relaxed),
                freed_bytes: total.freed_bytes + shard.freed_bytes.load(Ordering::Relaxed),
            })
    }
}

/// What [`AllocationCounter`] counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocationMetric {
    /// Allocations served, including reallocations: the churn put through the
    /// allocator, and nearly deterministic for the same work.
    Allocations,
    /// Bytes requested by those allocations.
    AllocatedBytes,
}

impl AllocationMetric {
    /// The unit noun used in reports.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Allocations => "allocations",
            Self::AllocatedBytes => "bytes",
        }
    }

    fn read(self, stats: &AllocationStats) -> u64 {
        match self {
            Self::Allocations => stats.allocations,
            Self::AllocatedBytes => stats.allocated_bytes,
        }
    }
}

/// A heap-allocation statistic over a set of threads. Needs [`CountingAllocator`]
/// installed as the binary's global allocator.
///
/// [`MetricScope::Process`] counts every thread. [`MetricScope::CallingThread`]
/// counts only the thread that runs the measurement. Background workers
/// allocate on their own schedule, so the calling thread's count is the
/// steadier regression gate.
pub struct AllocationCounter {
    metric: AllocationMetric,
    scope: MetricScope,
}

impl AllocationCounter {
    /// Creates a counter for `metric` over `scope`, failing when the binary's
    /// global allocator is not a [`CountingAllocator`].
    pub fn new(metric: AllocationMetric, scope: MetricScope) -> Result<Self> {
        if !allocation_counts::installed() {
            return Err(anyhow!(
                "this binary's global allocator is not bench_metrics::CountingAllocator"
            ));
        }
        Ok(Self { metric, scope })
    }

    /// The counted statistic.
    pub const fn metric(&self) -> AllocationMetric {
        self.metric
    }

    /// The covered threads.
    pub const fn scope(&self) -> MetricScope {
        self.scope
    }

    /// Reads the statistic so [`finish`](Self::finish) can report the delta.
    pub fn start(&self) -> Sample<u64> {
        Sample(self.read())
    }

    /// The count between `start` and now, in the metric's unit.
    pub fn finish(&self, start: Sample<u64>) -> f64 {
        self.read().saturating_sub(start.0) as f64
    }

    fn read(&self) -> u64 {
        let stats = match self.scope {
            MetricScope::Process => allocation_counts::read(),
            MetricScope::CallingThread => allocation_counts::read_calling_thread(),
        };
        self.metric.read(&stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_stats_absent_without_counting_allocator() {
        // The test binary uses the system allocator, so the counter must say so
        // rather than reporting a stale zero.
        assert!(allocation_stats().is_none());
        assert!(
            AllocationCounter::new(AllocationMetric::Allocations, MetricScope::Process).is_err()
        );
    }

    #[test]
    fn resource_counter_reads_and_advances() {
        let counter =
            ResourceCounter::new(ResourceMetric::MinorPageFaults, MetricScope::Process).unwrap();
        let start = counter.start();
        let delta = counter.finish(start);
        assert!(delta >= 0.0, "a page-fault delta cannot be negative");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hardware_counter_opens_or_reports_why_not() {
        // On a machine that forbids perf_event_open this returns Err; on one
        // that allows it, the counter must open and advance.
        match HardwareCounter::new(HardwareEvent::Instructions, MetricScope::CallingThread) {
            Ok(counter) => {
                let start = counter.start();
                let mut spin = 0_u64;
                for index in 0..10_000_u64 {
                    spin = spin.wrapping_add(index.rotate_left(3));
                }
                std::hint::black_box(spin);
                assert!(counter.finish(start) > 0.0);
            }
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("perf_event_paranoid") || message.contains("CAP_PERFMON"),
                    "an unavailable counter must explain how to enable it: {message}"
                );
            }
        }
    }
}
