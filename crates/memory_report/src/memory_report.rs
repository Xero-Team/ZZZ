//! Samples how much memory this process is using, for leak diagnosis.
//!
//! A leak in a long-running app rarely shows up as one large allocation; it
//! shows up as a line that drifts upward over hours. Detecting that needs two
//! numbers over time: what the operating system charges the process, and what
//! the allocator believes it is holding. The second one is what distinguishes a
//! leak from fragmentation or memory the OS has merely not reclaimed yet.
//!
//! Both numbers are cheap counters, so sampling them on an interval is safe in
//! a shipped build. Per-allocation stack capture is a different job: it costs
//! orders of magnitude more and belongs in a dedicated profiling build.

use anyhow::{Context as _, Result};
use human_bytes::human_bytes;
#[cfg(feature = "mimalloc")]
use serde::Deserialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// A point-in-time memory sample for the current process.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryReport {
    /// Resident bytes, i.e. the part of the address space backed by physical
    /// memory. Windows reports this as the working set.
    ///
    /// This is *not* the value macOS Activity Monitor shows. That figure is the
    /// physical footprint, which also counts compressed pages; see the note on
    /// [`AllocatorStatistics::peak_resident_bytes`] for the peak.
    pub resident_bytes: u64,
    /// Virtual address space size. On macOS this is dominated by reserved
    /// regions and is not a useful growth signal.
    pub virtual_bytes: u64,
    /// Allocator-side counters. Present only when the `mimalloc` feature is
    /// enabled and mimalloc is the process's global allocator.
    pub allocator: Option<AllocatorStatistics>,
}

/// Allocator-side heap counters.
///
/// These come from mimalloc's own statistics, which it maintains as it
/// allocates; reading them adds no per-allocation cost.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AllocatorStatistics {
    /// Peak resident bytes since process start. Unlike
    /// [`MemoryReport::resident_bytes`] this is a high-water mark, so it can
    /// only answer "how bad did it get", never "is it still growing".
    pub peak_resident_bytes: u64,
    /// Bytes committed from the OS for heap segments. This is the number to
    /// watch for a leak: it rises when live allocations outgrow what the
    /// allocator already holds and falls only when segments are purged.
    pub committed_bytes: u64,
    /// Peak of [`Self::committed_bytes`].
    pub peak_committed_bytes: u64,
    /// Address space reserved for future heap segments. Large and mostly
    /// constant; not a growth signal.
    pub reserved_bytes: u64,
    /// Heap pages currently in use, at mimalloc's page size.
    pub pages: u64,
}

impl MemoryReport {
    /// Samples the current process once.
    ///
    /// Prefer [`MemoryReporter`] when sampling repeatedly: it keeps the
    /// [`System`] handle that sysinfo needs to compute a process snapshot.
    pub fn current() -> Result<Self> {
        MemoryReporter::new()?.report()
    }

    /// Renders the report as one `key=value` line for a metrics scraper.
    ///
    /// The tag matches what `cargo perf-soak` parses, so a workload that prints
    /// this line per interval can be compared for upward drift.
    pub fn to_metrics_line(&self) -> String {
        format!("ZZZ_MEMORY {}", self.metrics_fields())
    }

    /// The `key=value` fields of [`Self::to_metrics_line`] without the tag, for
    /// callers that add fields of their own after it.
    pub fn metrics_fields(&self) -> String {
        let mut line = format!(
            "resident_bytes={} virtual_bytes={}",
            self.resident_bytes, self.virtual_bytes
        );
        if let Some(allocator) = &self.allocator {
            line.push_str(&format!(
                " heap_peak_resident_bytes={} heap_committed_bytes={} \
                 heap_peak_committed_bytes={} heap_reserved_bytes={} heap_pages={}",
                allocator.peak_resident_bytes,
                allocator.committed_bytes,
                allocator.peak_committed_bytes,
                allocator.reserved_bytes,
                allocator.pages
            ));
        }
        line
    }
}

impl std::fmt::Display for MemoryReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "resident {}, virtual {}",
            format_bytes(self.resident_bytes),
            format_bytes(self.virtual_bytes)
        )?;
        if let Some(allocator) = &self.allocator {
            write!(
                f,
                ", heap committed {} (peak {}), reserved {}, {} pages, peak resident {}",
                format_bytes(allocator.committed_bytes),
                format_bytes(allocator.peak_committed_bytes),
                format_bytes(allocator.reserved_bytes),
                allocator.pages,
                format_bytes(allocator.peak_resident_bytes),
            )?;
        }
        Ok(())
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Byte counts are rounded for display; f64's 53-bit mantissa keeps them \
              well past the precision `human_bytes` prints."
)]
fn format_bytes(bytes: u64) -> String {
    human_bytes(bytes as f64)
}

/// Samples the current process repeatedly.
///
/// Holding one [`System`] across samples is what sysinfo asks for: a fresh
/// instance re-reads process metadata that this type already has.
#[derive(Debug)]
pub struct MemoryReporter {
    system: System,
    pid: Pid,
}

impl MemoryReporter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            system: System::new().context("could not read system information")?,
            pid: sysinfo::get_current_pid().context("could not resolve the current pid")?,
        })
    }

    /// Samples memory once. The `System` is refreshed for this pid alone, so the
    /// cost does not grow with the number of processes on the machine.
    pub fn report(&mut self) -> Result<MemoryReport> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            false,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let process = self
            .system
            .process(self.pid)
            .context("the current process is missing from the system snapshot")?;

        Ok(MemoryReport {
            resident_bytes: process.memory(),
            virtual_bytes: process.virtual_memory(),
            allocator: allocator_statistics(),
        })
    }
}

/// Asks the allocator to return unused heap pages to the operating system.
///
/// mimalloc purges on a delay that further allocations keep refreshing, so a
/// process that goes quiet can hold committed memory it no longer needs: a
/// freed 192 MiB block stayed resident until an explicit collect in
/// [microsoft/mimalloc#1422](https://github.com/microsoft/mimalloc/issues/1422).
/// A non-forced collect purges the calling thread's free pages and any range
/// already due, which is cheap enough for a quiescent process and too expensive
/// to run while allocations are in flight.
///
/// Returns `false` when mimalloc is not the global allocator, where there is
/// nothing of ours to release.
pub fn release_unused_memory() -> bool {
    #[cfg(feature = "mimalloc")]
    {
        // SAFETY: `mi_collect` takes no pointer and is documented as callable
        // from any thread; `false` requests the non-forced collection that only
        // touches pages this thread owns plus ranges already scheduled for
        // purge. The safe equivalent would be setting `MIMALLOC_PURGE_DELAY=0`,
        // which is process-wide and purges on every free instead of on demand.
        unsafe { libmimalloc_sys::mi_collect(false) };
        true
    }
    #[cfg(not(feature = "mimalloc"))]
    {
        false
    }
}

/// One size bucket of live heap blocks, covering `[min_block_bytes, 2 * min)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlockBucket {
    /// Smallest block size in the bucket.
    pub min_block_bytes: u64,
    pub blocks: u64,
    pub bytes: u64,
}

impl std::fmt::Display for BlockBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "≥{}: {} blocks, {}",
            format_bytes(self.min_block_bytes),
            self.blocks,
            format_bytes(self.bytes)
        )
    }
}

/// Buckets the live heap blocks of the calling thread by size.
///
/// This separates "many small objects are being retained" from "a few large
/// buffers are being retained", which the aggregate counters cannot tell apart.
///
/// mimalloc v3 gives every thread its own heap and the public API exposes only
/// the calling thread's, so blocks allocated elsewhere are not counted. For an
/// editor that still covers document and UI state, which is allocated where it
/// is rendered.
///
/// The walk is linear in the number of live blocks, so this belongs behind an
/// explicit request rather than on a timer. It is also approximate: mimalloc
/// visits the blocks of pages the calling thread's heap still tracks, so a fresh
/// or partly-reclaimed heap reports a subset. Read it for the shape of the
/// distribution, not as an exact accounting. Returns `None` when mimalloc is not
/// the allocator in use.
pub fn block_size_distribution() -> Option<Vec<BlockBucket>> {
    #[cfg(feature = "mimalloc")]
    {
        let mut histogram = Histogram::default();
        // SAFETY: `mi_theap_get_default` returns this thread's heap, or null
        // when mimalloc has not initialised one yet, which is checked below.
        // `arg` points at `histogram`, which outlives the walk, and the visitor
        // only ever dereferences it on this thread during the call.
        let theap = unsafe { mi_theap_get_default() };
        if theap.is_null() {
            return None;
        }

        // SAFETY: `theap` came from `mi_theap_get_default` above and is the
        // thread's own heap; `record_block` matches `mi_block_visit_fun`'s
        // signature and never stops the walk early.
        unsafe {
            mi_theap_visit_blocks(
                theap,
                true,
                Some(record_block),
                std::ptr::from_mut(&mut histogram).cast(),
            );
        }

        Some(histogram.into_buckets())
    }
    #[cfg(not(feature = "mimalloc"))]
    {
        None
    }
}

/// The `limit` buckets holding the most bytes, largest first.
///
/// A bucket's minimum size is not its weight: ten megabytes of 4 KiB blocks
/// matter more than one megabyte of 1 MiB blocks, and only this ordering shows
/// that.
#[must_use]
pub fn largest_buckets(buckets: &[BlockBucket], limit: usize) -> Vec<BlockBucket> {
    let mut sorted = buckets.to_vec();
    sorted.sort_by_key(|bucket| std::cmp::Reverse(bucket.bytes));
    sorted.truncate(limit);
    sorted.retain(|bucket| bucket.bytes > 0);
    sorted
}

#[cfg(feature = "mimalloc")]
#[derive(Default)]
struct Histogram {
    buckets: std::collections::BTreeMap<u32, (u64, u64)>,
}

#[cfg(feature = "mimalloc")]
impl Histogram {
    fn into_buckets(self) -> Vec<BlockBucket> {
        self.buckets
            .into_iter()
            .map(|(exponent, (blocks, bytes))| BlockBucket {
                min_block_bytes: 1u64 << exponent,
                blocks,
                bytes,
            })
            .collect()
    }
}

/// The power-of-two bucket a block size falls in.
#[cfg(feature = "mimalloc")]
fn bucket_exponent(block_size: usize) -> u32 {
    usize::BITS - 1 - block_size.leading_zeros()
}

#[cfg(feature = "mimalloc")]
unsafe extern "C" fn record_block(
    _heap: *const libmimalloc_sys::mi_heap_t,
    _area: *const libmimalloc_sys::mi_heap_area_t,
    block: *mut core::ffi::c_void,
    block_size: usize,
    arg: *mut core::ffi::c_void,
) -> bool {
    // SAFETY: `arg` is the `&mut Histogram` that `block_size_distribution` keeps
    // alive across the walk, and mimalloc calls this visitor on the same thread
    // inside that call with no aliasing access to it.
    let histogram = unsafe { &mut *arg.cast::<Histogram>() };
    if !block.is_null() && block_size > 0 {
        let entry = histogram
            .buckets
            .entry(bucket_exponent(block_size))
            .or_default();
        entry.0 += 1;
        entry.1 += block_size as u64;
    }
    // Never stop early: the whole distribution is the point of the walk.
    true
}

/// mimalloc v3's per-thread heap handle.
#[cfg(feature = "mimalloc")]
#[repr(C)]
struct MiTheap {
    _private: [u8; 0],
}

#[cfg(feature = "mimalloc")]
unsafe extern "C" {
    /// The calling thread's heap. `libmimalloc-sys` binds v2's
    /// `mi_heap_get_default` behind its `v2` feature but never binds this v3
    /// accessor, so the two declarations below come from
    /// `c_src/mimalloc/v3/include/mimalloc.h` instead.
    fn mi_theap_get_default() -> *mut MiTheap;
    fn mi_theap_visit_blocks(
        theap: *const MiTheap,
        visit_blocks: bool,
        visitor: libmimalloc_sys::mi_block_visit_fun,
        arg: *mut core::ffi::c_void,
    ) -> bool;
}

#[cfg(feature = "mimalloc")]
fn allocator_statistics() -> Option<AllocatorStatistics> {
    let stats = mimalloc::MiMalloc::stats_json()
        .inspect_err(|error| log::debug!("could not read mimalloc statistics: {error}"))
        .ok()?;
    let stats: MimallocStats = serde_json::from_str(stats.to_str().ok()?)
        .inspect_err(|error| log::debug!("could not parse mimalloc statistics: {error}"))
        .ok()?;
    Some(AllocatorStatistics {
        peak_resident_bytes: stats.process.rss_peak,
        committed_bytes: stats.committed.current,
        peak_committed_bytes: stats.committed.peak,
        reserved_bytes: stats.reserved.current,
        pages: stats.pages.current,
    })
}

#[cfg(not(feature = "mimalloc"))]
fn allocator_statistics() -> Option<AllocatorStatistics> {
    None
}

/// The subset of `mi_stats_get_json` this crate reads.
///
/// mimalloc calls it `stat_version` 5; a newer major schema would drop fields
/// this struct requires and be reported as a parse error rather than a silent
/// zero.
#[cfg(feature = "mimalloc")]
#[derive(Debug, Deserialize)]
struct MimallocStats {
    process: MimallocProcessStatistics,
    pages: MimallocCounter,
    reserved: MimallocCounter,
    committed: MimallocCounter,
}

#[cfg(feature = "mimalloc")]
#[derive(Debug, Deserialize)]
struct MimallocProcessStatistics {
    rss_peak: u64,
}

#[cfg(feature = "mimalloc")]
#[derive(Debug, Deserialize)]
struct MimallocCounter {
    current: u64,
    peak: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        AllocatorStatistics, BlockBucket, MemoryReport, MemoryReporter, block_size_distribution,
        largest_buckets,
    };

    fn sample() -> MemoryReport {
        MemoryReport {
            resident_bytes: 1_048_576,
            virtual_bytes: 4_294_967_296,
            allocator: Some(AllocatorStatistics {
                peak_resident_bytes: 2_097_152,
                committed_bytes: 524_288,
                peak_committed_bytes: 1_048_576,
                reserved_bytes: 2_147_483_648,
                pages: 8,
            }),
        }
    }

    #[test]
    fn sampling_reports_a_nonzero_footprint() {
        let report = MemoryReporter::new()
            .expect("reporter")
            .report()
            .expect("report");

        // Any process running this test holds at least its own executable
        // mappings, so a zero here means the sampling path is broken rather
        // than that the process is unusually small.
        assert!(report.resident_bytes > 0, "{report:?}");
        assert!(report.virtual_bytes >= report.resident_bytes, "{report:?}");
    }

    #[test]
    fn metrics_line_carries_every_key_a_scraper_reads() {
        let line = sample().to_metrics_line();
        let mut fields = line.split_whitespace();
        assert_eq!(fields.next(), Some("ZZZ_MEMORY"));

        let keys = fields
            .map(|field| field.split_once('=').expect("key=value"))
            .collect::<Vec<_>>();
        assert_eq!(
            keys.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            vec![
                "resident_bytes",
                "virtual_bytes",
                "heap_peak_resident_bytes",
                "heap_committed_bytes",
                "heap_peak_committed_bytes",
                "heap_reserved_bytes",
                "heap_pages",
            ]
        );
        for (key, value) in keys {
            assert!(
                value.parse::<f64>().is_ok(),
                "{key} must be numeric for perf-soak, got {value:?}"
            );
        }
    }

    #[test]
    fn metrics_line_omits_allocator_keys_without_an_allocator() {
        let mut report = sample();
        report.allocator = None;

        let line = report.to_metrics_line();
        assert!(line.contains("resident_bytes="));
        assert!(!line.contains("heap_committed_bytes="), "{line}");
    }

    #[test]
    fn largest_buckets_orders_by_bytes_not_by_block_size() {
        let buckets = vec![
            BlockBucket {
                min_block_bytes: 1_048_576,
                blocks: 1,
                bytes: 1_048_576,
            },
            BlockBucket {
                min_block_bytes: 4_096,
                blocks: 2_560,
                bytes: 10_485_760,
            },
            BlockBucket {
                min_block_bytes: 64,
                blocks: 10,
                bytes: 640,
            },
        ];

        let largest = largest_buckets(&buckets, 2);

        assert_eq!(largest.len(), 2);
        assert_eq!(largest[0].min_block_bytes, 4_096, "most bytes first");
        assert_eq!(largest[1].min_block_bytes, 1_048_576);
    }

    #[cfg(feature = "mimalloc")]
    #[test]
    fn block_distribution_sees_live_allocations() {
        use std::alloc::{GlobalAlloc as _, Layout};

        // Allocate through mimalloc directly rather than through the global
        // allocator: this test binary uses the system allocator, so a plain
        // `Vec` would never reach the heap being walked.
        //
        // Small blocks on purpose: mimalloc serves objects above the page size
        // (64 KiB by default) outside the pages the visitor walks, so a large
        // allocation would legitimately not appear here.
        const BLOCKS: usize = 1_000;
        let layout = Layout::from_size_align(256, 8).expect("valid layout");
        let mut pointers = Vec::with_capacity(BLOCKS);
        for _ in 0..BLOCKS {
            // SAFETY: the layout is non-zero sized, which `GlobalAlloc::alloc` requires.
            let pointer = unsafe { mimalloc::MiMalloc.alloc(layout) };
            assert!(!pointer.is_null(), "mimalloc should allocate 256 bytes");
            pointers.push(pointer);
        }

        let buckets = block_size_distribution().expect("mimalloc heap should be visitable");
        let total_bytes: u64 = buckets.iter().map(|bucket| bucket.bytes).sum();
        let total_blocks: u64 = buckets.iter().map(|bucket| bucket.blocks).sum();

        for pointer in pointers {
            // SAFETY: every pointer came from `MiMalloc::alloc` with this layout.
            unsafe { mimalloc::MiMalloc.dealloc(pointer, layout) };
        }

        // The walk reports a subset of live blocks, not all of them: mimalloc
        // only visits blocks in pages its theap still tracks, and a freshly
        // grown heap can hold pages it has not linked yet. A non-empty result is
        // what proves the sample is wired to mimalloc at all.
        assert!(
            total_blocks > 0 && total_bytes > 0,
            "the walk should see this thread's small blocks, saw {total_blocks} \
             blocks / {total_bytes} bytes"
        );
        assert!(
            buckets
                .windows(2)
                .all(|pair| pair[0].min_block_bytes < pair[1].min_block_bytes),
            "buckets come back in ascending size order"
        );
    }

    #[test]
    fn parse_sample_elapsed_is_not_this_crates_job() {
        // Guard against the perf helper drifting into this crate: the metrics
        // line must stay tag-only so the scraper owns the timestamp.
        assert!(!sample().to_metrics_line().contains("elapsed_secs"));
    }
}
