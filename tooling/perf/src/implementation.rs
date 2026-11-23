//! The implementation of the this crate is kept in a separate module
//! so that it is easy to publish this crate as part of GPUI's dependencies

use collections::HashMap;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, num::NonZero, time::Duration};

pub mod consts {
    //! Preset identifiers and constants so that the profiler and proc macro agree
    //! on their communication protocol.

    /// The suffix on the actual test function.
    pub const SUF_NORMAL: &str = "__ZZZ_PERF_FN";
    /// The suffix on an extra function which prints metadata about a test to stdout.
    pub const SUF_MDATA: &str = "__ZZZ_PERF_MDATA";
    /// The env var in which we pass the iteration count to our tests.
    pub const ITER_ENV_VAR: &str = "ZZZ_PERF_ITER";
    /// The prefix printed on all benchmark test metadata lines, to distinguish it from
    /// possible output by the test harness itself.
    pub const MDATA_LINE_PREF: &str = "ZZZ_MDATA_";
    /// The version number for the data returned from the test metadata function.
    /// Increment on non-backwards-compatible changes.
    pub const MDATA_VER: u32 = 0;
    /// The default weight, if none is specified.
    pub const WEIGHT_DEFAULT: u8 = 50;
    /// How long a test must have run to be assumed to be reliable-ish.
    pub const NOISE_CUTOFF: std::time::Duration = std::time::Duration::from_millis(250);

    /// Identifier for the iteration count of a test metadata.
    pub const ITER_COUNT_LINE_NAME: &str = "iter_count";
    /// Identifier for the weight of a test metadata.
    pub const WEIGHT_LINE_NAME: &str = "weight";
    /// Identifier for importance in test metadata.
    pub const IMPORTANCE_LINE_NAME: &str = "importance";
    /// Identifier for the test metadata version.
    pub const VERSION_LINE_NAME: &str = "version";

    /// Where to save json run information.
    pub const RUNS_DIR: &str = ".perf-runs";
    /// The A/A noise floor recorded by `cargo perf-compare noise`, relative to
    /// `RUNS_DIR`.
    pub const NOISE_FLOOR_FILE: &str = "noise.floor.json";
}

/// The smallest per-test change that counts as real when no noise floor says
/// otherwise.
///
/// Two runs of one commit on one machine move by a few percent; below this, a
/// delta is measurement noise rather than signal. `compare` still prints such
/// tests, it just does not let them move the category totals.
pub const DEFAULT_SIGNIFICANCE_THRESHOLD: f64 = 0.02;

/// How relevant a benchmark is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Importance {
    /// Regressions shouldn't be accepted without good reason.
    Critical = 4,
    /// Regressions should be paid extra attention.
    Important = 3,
    /// No extra attention should be paid to regressions, but they might still
    /// be indicative of something happening.
    #[default]
    Average = 2,
    /// Unclear if regressions are likely to be meaningful, but still worth keeping
    /// an eye on. Lowest level that's checked by default by the profiler.
    Iffy = 1,
    /// Regressions are likely to be spurious or don't affect core functionality.
    /// Only relevant if a lot of them happen, or as supplemental evidence for a
    /// higher-importance benchmark regressing. Not checked by default.
    Fluff = 0,
}

impl std::fmt::Display for Importance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Importance::Critical => f.write_str("critical"),
            Importance::Important => f.write_str("important"),
            Importance::Average => f.write_str("average"),
            Importance::Iffy => f.write_str("iffy"),
            Importance::Fluff => f.write_str("fluff"),
        }
    }
}

/// Why or when did this test fail?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FailKind {
    /// Failed while triaging it to determine the iteration count.
    Triage,
    /// Failed while profiling it.
    Profile,
    /// Failed due to an incompatible version for the test.
    VersionMismatch,
    /// Could not parse metadata for a test.
    BadMetadata,
    /// Skipped due to filters applied on the perf run.
    Skipped,
}

impl std::fmt::Display for FailKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailKind::Triage => f.write_str("errored in triage"),
            FailKind::Profile => f.write_str("errored while profiling"),
            FailKind::VersionMismatch => f.write_str("test version mismatch"),
            FailKind::BadMetadata => f.write_str("bad test metadata"),
            FailKind::Skipped => f.write_str("skipped"),
        }
    }
}

/// Information about a given perf test.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestMdata {
    /// A version number for when the test was generated. If this is greater
    /// than the version this test handler expects, one of the following will
    /// happen in an unspecified manner:
    /// - The test is skipped silently.
    /// - The handler exits with an error message indicating the version mismatch
    ///   or inability to parse the metadata.
    ///
    /// INVARIANT: If `version` <= `MDATA_VER`, this tool *must* be able to
    /// correctly parse the output of this test.
    pub version: u32,
    /// How many iterations to pass this test if this is preset, or how many
    /// iterations a test ended up running afterwards if determined at runtime.
    pub iterations: Option<NonZero<usize>>,
    /// The importance of this particular test. See the docs on `Importance` for
    /// details.
    pub importance: Importance,
    /// The weight of this particular test within its importance category. Used
    /// when comparing across runs.
    pub weight: u8,
}

/// The actual timings of a test, as measured by Hyperfine.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Timings {
    /// Mean runtime for `self.iter_total` runs of this test.
    pub mean: Duration,
    /// Standard deviation for the above.
    pub stddev: Duration,
}

impl Timings {
    /// How many iterations does this test seem to do per second?
    #[expect(
        clippy::cast_precision_loss,
        reason = "We only care about a couple sig figs anyways"
    )]
    #[must_use]
    pub fn iters_per_sec(&self, total_iters: NonZero<usize>) -> f64 {
        (1000. / self.mean.as_millis() as f64) * total_iters.get() as f64
    }
}

/// Aggregate results, meant to be used for a given importance category. Each
/// test name corresponds to its benchmark results, iteration count, and weight.
type CategoryInfo = HashMap<String, (Timings, NonZero<usize>, u8)>;

/// Aggregate output of all tests run by this handler.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Output {
    /// A list of test outputs. Format is `(test_name, mdata, timings)`.
    /// The latter being `Ok(_)` indicates the test succeeded.
    ///
    /// INVARIANT: If the test succeeded, the second field is `Some(mdata)` and
    /// `mdata.iterations` is `Some(_)`.
    tests: Vec<(String, Option<TestMdata>, Result<Timings, FailKind>)>,
    /// The build profile this run was collected under, derived from the test
    /// binary path (e.g. `release-fast`).
    ///
    /// `compare` refuses to diff two runs whose profiles differ, because a
    /// debug-versus-optimized delta says nothing about a change to the code.
    /// Older run files predate this field and deserialize as `None`.
    #[serde(default)]
    profile: Option<String>,
}

impl Output {
    /// Instantiates an empty "output". Useful for merging.
    #[must_use]
    pub fn blank() -> Self {
        Output {
            tests: Vec::new(),
            profile: None,
        }
    }

    /// Records the build profile this run was collected under.
    pub fn set_profile(&mut self, profile: Option<String>) {
        self.profile = profile;
    }

    /// The build profile this run was collected under, if it was recorded.
    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    /// Reports a success and adds it to this run's `Output`.
    pub fn success(
        &mut self,
        name: impl AsRef<str>,
        mut mdata: TestMdata,
        iters: NonZero<usize>,
        timings: Timings,
    ) {
        mdata.iterations = Some(iters);
        self.tests
            .push((name.as_ref().to_owned(), Some(mdata), Ok(timings)));
    }

    /// Reports a failure and adds it to this run's `Output`. If this test was tried
    /// with some number of iterations (i.e. this was not a version mismatch or skipped
    /// test), it should be reported also.
    ///
    /// Using the `fail!()` macro is usually more convenient.
    pub fn failure(
        &mut self,
        name: impl AsRef<str>,
        mut mdata: Option<TestMdata>,
        attempted_iters: Option<NonZero<usize>>,
        kind: FailKind,
    ) {
        if let Some(ref mut mdata) = mdata {
            mdata.iterations = attempted_iters;
        }
        self.tests
            .push((name.as_ref().to_owned(), mdata, Err(kind)));
    }

    /// True if no tests executed this run.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }

    /// Sorts the runs in the output in the order that we want them printed.
    pub fn sort(&mut self) {
        self.tests.sort_unstable_by(|a, b| match (a, b) {
            // Tests where we got no metadata go at the end.
            ((_, Some(_), _), (_, None, _)) => std::cmp::Ordering::Greater,
            ((_, None, _), (_, Some(_), _)) => std::cmp::Ordering::Less,
            // Then sort by importance, then weight.
            ((_, Some(a_mdata), _), (_, Some(b_mdata), _)) => {
                let c = a_mdata.importance.cmp(&b_mdata.importance);
                if matches!(c, std::cmp::Ordering::Equal) {
                    a_mdata.weight.cmp(&b_mdata.weight)
                } else {
                    c
                }
            }
            // Lastly by name.
            ((a_name, ..), (b_name, ..)) => a_name.cmp(b_name),
        });
    }

    /// Merges the output of two runs, appending a prefix to the results of the new run.
    /// To be used in conjunction with `Output::blank()`, or else only some tests will have
    /// a prefix set.
    pub fn merge<'a>(&mut self, other: Self, pref_other: impl Into<Option<&'a str>>) {
        if self.profile.is_none() {
            self.profile = other.profile;
        }
        let pref = if let Some(pref) = pref_other.into() {
            "crates/".to_owned() + pref + "::"
        } else {
            String::new()
        };
        self.tests = std::mem::take(&mut self.tests)
            .into_iter()
            .chain(
                other
                    .tests
                    .into_iter()
                    .map(|(name, md, tm)| (pref.clone() + &name, md, tm)),
            )
            .collect();
    }

    /// The throughput of every successful test, in iterations per second.
    fn throughputs(&self) -> HashMap<&str, f64> {
        self.tests
            .iter()
            .filter_map(|(name, mdata, timings)| {
                let mdata = mdata.as_ref()?;
                let timings = timings.as_ref().ok()?;
                let iterations = mdata.iterations?;
                Some((name.as_str(), timings.iters_per_sec(iterations)))
            })
            .collect()
    }

    /// Evaluates the performance of `self` against `baseline`. The latter is taken
    /// as the comparison point, i.e. a positive resulting `PerfReport` means that
    /// `self` performed better.
    ///
    /// A test counts towards its category only when its change clears both
    /// `threshold` and the recorded A/A `floor` for that test, so measurement
    /// noise cannot masquerade as a regression. Tests below the gate are still
    /// counted per category and reported as suppressed.
    ///
    /// # Panics
    /// `self` and `baseline` are assumed to have the iterations field on all
    /// `TestMdata`s set to `Some(_)` if the `TestMdata` is present itself.
    #[must_use]
    pub fn compare_perf(
        self,
        baseline: Self,
        floor: Option<&NoiseFloor>,
        threshold: f64,
    ) -> PerfReport {
        let self_categories = self.collapse();
        let mut other_categories = baseline.collapse();

        let deltas = self_categories
            .into_iter()
            .filter_map(|(cat, self_data)| {
                // Only compare categories where both           meow
                // runs have data.                              /
                let mut other_data = other_categories.remove(&cat)?;
                let mut max = f64::MIN;
                let mut min = f64::MAX;

                // Running totals for averaging out tests.
                let mut r_total_numerator = 0.;
                let mut r_total_denominator = 0;
                let mut counted = 0;
                let mut suppressed = 0;
                // Yeah this is O(n^2), but realistically it'll hardly be a bottleneck.
                for (name, (s_timings, s_iters, weight)) in self_data {
                    // Only use the new weights if they conflict.
                    let Some((o_timings, o_iters, _)) = other_data.remove(&name) else {
                        continue;
                    };
                    let shift =
                        (o_timings.iters_per_sec(o_iters) / s_timings.iters_per_sec(s_iters)) - 1.;
                    let gate = threshold.max(floor.map_or(0.0, |floor| floor.spread_for(&name)));
                    if shift.abs() < gate {
                        suppressed += 1;
                        continue;
                    }
                    if shift > max {
                        max = shift;
                    }
                    if shift < min {
                        min = shift;
                    }
                    r_total_numerator += shift * f64::from(weight);
                    r_total_denominator += u32::from(weight);
                    counted += 1;
                }
                if counted == 0 {
                    // Nothing cleared the noise gate, but the category did exist
                    // in both runs; say so instead of pretending it was absent.
                    Some((
                        cat,
                        PerfDelta {
                            max: 0.,
                            mean: 0.,
                            min: 0.,
                            counted,
                            suppressed,
                        },
                    ))
                } else {
                    let mean = r_total_numerator / f64::from(r_total_denominator);
                    // TODO: also aggregate standard deviation? That's harder to keep
                    // meaningful, though, since we dk which tests are correlated.
                    Some((
                        cat,
                        PerfDelta {
                            max,
                            mean,
                            min,
                            counted,
                            suppressed,
                        },
                    ))
                }
            })
            .collect();

        PerfReport { deltas }
    }

    /// Collapses the `PerfReport` into a `HashMap` over `Importance`, with
    /// each importance category having its tests contained.
    fn collapse(self) -> HashMap<Importance, CategoryInfo> {
        let mut categories = HashMap::<Importance, HashMap<String, _>>::default();
        for entry in self.tests {
            if let Some(mdata) = entry.1
                && let Ok(timings) = entry.2
            {
                if let Some(handle) = categories.get_mut(&mdata.importance) {
                    handle.insert(
                        entry.0,
                        (
                            timings,
                            mdata
                                .iterations
                                .expect("invariant: iterations is Some for a successful test"),
                            mdata.weight,
                        ),
                    );
                } else {
                    let mut new = HashMap::default();
                    new.insert(
                        entry.0,
                        (
                            timings,
                            mdata
                                .iterations
                                .expect("invariant: iterations is Some for a successful test"),
                            mdata.weight,
                        ),
                    );
                    categories.insert(mdata.importance, new);
                }
            }
        }

        categories
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print the header for an empty run.
        if self.tests.is_empty() {
            return Ok(());
        }

        // We want to print important tests at the top, then alphabetical.
        let mut sorted = self.clone();
        sorted.sort();
        // Markdown header for making a nice little table :>
        writeln!(
            f,
            "| Command | Iter/sec | Mean [ms] | SD [ms] | Iterations | Importance (weight) |",
        )?;
        writeln!(f, "|:---|---:|---:|---:|---:|---:|")?;
        for (name, metadata, timings) in &sorted.tests {
            match metadata {
                Some(metadata) => match timings {
                    // Happy path.
                    Ok(timings) => {
                        // If the test succeeded, then metadata.iterations is Some(_).
                        writeln!(
                            f,
                            "| {} | {:.2} | {} | {:.2} | {} | {} ({}) |",
                            name,
                            timings.iters_per_sec(
                                metadata
                                    .iterations
                                    .expect("invariant: iterations is Some for a successful test"),
                            ),
                            {
                                // Very small mean runtimes will give inaccurate
                                // results. Should probably also penalise weight.
                                let mean = timings.mean.as_secs_f64() * 1000.;
                                if mean < consts::NOISE_CUTOFF.as_secs_f64() * 1000. / 8. {
                                    format!("{mean:.2} (unreliable)")
                                } else {
                                    format!("{mean:.2}")
                                }
                            },
                            timings.stddev.as_secs_f64() * 1000.,
                            metadata
                                .iterations
                                .expect("invariant: iterations is Some for a successful test"),
                            metadata.importance,
                            metadata.weight,
                        )?;
                    }
                    // We have (some) metadata, but the test errored.
                    Err(err) => writeln!(
                        f,
                        "| ({}) {} | N/A | N/A | N/A | {} | {} ({}) |",
                        err,
                        name,
                        metadata
                            .iterations
                            .map_or_else(|| "N/A".to_owned(), |i| format!("{i}")),
                        metadata.importance,
                        metadata.weight
                    )?,
                },
                // No metadata, couldn't even parse the test output. A missing
                // metadata always comes with a failure, but if a deserialized
                // run ever pairs `None` with `Ok`, report it as bad metadata
                // instead of panicking.
                None => writeln!(
                    f,
                    "| ({}) {name} | N/A | N/A | N/A | N/A | N/A |",
                    match timings {
                        Err(failure) => failure.clone(),
                        Ok(_) => FailKind::BadMetadata,
                    }
                )?,
            }
        }
        Ok(())
    }
}

/// The difference in performance between two runs within a given importance
/// category.
struct PerfDelta {
    /// The biggest improvement / least bad regression.
    ///
    /// Zero when every test in the category fell below the noise gate.
    max: f64,
    /// The weighted average change in test times.
    mean: f64,
    /// The worst regression / smallest improvement.
    ///
    /// Zero when every test in the category fell below the noise gate.
    min: f64,
    /// How many tests cleared the noise gate and shaped this row.
    counted: usize,
    /// How many tests were inside the gate and excluded from this row.
    suppressed: usize,
}

/// Per-test measurement noise, measured by running one checkout twice (`A/A`).
///
/// `compare` gates every test on its own recorded spread, so a metric that moves
/// by 4% on an idle machine is not reported as a 4% regression. A test absent
/// from the floor (new test, different machine) falls back to
/// [`DEFAULT_SIGNIFICANCE_THRESHOLD`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NoiseFloor {
    /// The largest relative spread observed per test name, keyed the same way
    /// `compare` names tests (crate-prefixed).
    per_test: BTreeMap<String, f64>,
    /// The build profile both runs were collected under.
    #[serde(default)]
    profile: Option<String>,
}

impl NoiseFloor {
    /// Measures the per-test spread between two runs of the same checkout.
    ///
    /// Both runs must have been collected under the same build profile; the
    /// caller is responsible for refusing a mixed pair before calling this.
    #[must_use]
    pub fn measure(first: &Output, second: &Output) -> Self {
        let first_throughputs = first.throughputs();
        let second_throughputs = second.throughputs();

        let mut per_test = BTreeMap::new();
        for (name, first_throughput) in &first_throughputs {
            let Some(second_throughput) = second_throughputs.get(name) else {
                continue;
            };
            if *first_throughput <= 0. || *second_throughput <= 0. {
                continue;
            }
            let spread = (first_throughput / second_throughput - 1.).abs();
            per_test.insert((*name).to_owned(), spread);
        }

        Self {
            per_test,
            profile: first
                .profile()
                .or_else(|| second.profile())
                .map(ToOwned::to_owned),
        }
    }

    /// The relative spread recorded for `test`, or zero if it was not measured.
    #[must_use]
    pub fn spread_for(&self, test: &str) -> f64 {
        self.per_test.get(test).copied().unwrap_or(0.)
    }

    /// The build profile both runs were collected under, if it was recorded.
    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    /// True when no test was measured, e.g. because the two runs shared no names.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_test.is_empty()
    }
}

impl std::fmt::Display for NoiseFloor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.per_test.is_empty() {
            return write!(f, "(no shared tests between the two runs)");
        }
        let mut sorted = self.per_test.iter().collect::<Vec<_>>();
        sorted.sort_unstable_by(|(_, left), (_, right)| {
            right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal)
        });
        writeln!(f, "| Test | A/A spread |")?;
        write!(f, "|:---|---:|")?;
        for (test, spread) in sorted {
            write!(f, "\n| {test} | {:.1}% |", spread * 100.)?;
        }
        Ok(())
    }
}

/// Shim type for reporting all performance deltas across importance categories.
pub struct PerfReport {
    /// Inner (group, diff) pairing.
    deltas: HashMap<Importance, PerfDelta>,
}

/// Parses the numeric `key=value` fields out of one line of workload output.
///
/// The leading token names the emitting phase (for example
/// `EDITOR_FRAME_WORKLOAD`). A `workload=<name>` field distinguishes repeated
/// phases inside one run, so it is folded into the metric name instead of being
/// reported as a metric itself. Values that do not parse as a float are ignored,
/// which lets a workload print labels alongside its numbers.
#[must_use]
pub fn parse_metric_line(line: &str) -> Vec<(String, f64)> {
    let mut tokens = line.split_whitespace();
    let Some(tag) = tokens.next() else {
        return Vec::new();
    };
    if tag.contains('=') {
        return Vec::new();
    }

    let fields = tokens
        .filter_map(|token| token.split_once('='))
        .collect::<Vec<_>>();
    let workload = fields
        .iter()
        .find_map(|(key, value)| (*key == "workload").then_some(*value));
    let prefix = match workload {
        Some(workload) => format!("{tag}[{workload}]"),
        None => tag.to_owned(),
    };

    fields
        .into_iter()
        .filter(|(key, _)| *key != "workload")
        .filter_map(|(key, value)| {
            let value = value.parse::<f64>().ok()?;
            Some((format!("{prefix}.{key}"), value))
        })
        .collect()
}

/// The least-squares slope of `samples`, in units of y per unit of x.
///
/// Returns `None` when there are fewer than two samples or every sample shares
/// one x value, i.e. when a slope would be meaningless.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "Sample counts are small enough that f64 represents them exactly"
)]
pub fn linear_slope(samples: &[(f64, f64)]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let count = samples.len() as f64;
    let mean_x = samples.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = samples.iter().map(|(_, y)| y).sum::<f64>() / count;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (x, y) in samples {
        covariance += (x - mean_x) * (y - mean_y);
        variance += (x - mean_x) * (x - mean_x);
    }
    (variance > 0.0).then(|| covariance / variance)
}

/// One metric's samples over a soak run, as `(elapsed minutes, value)`.
pub type SoakSamples = BTreeMap<String, Vec<(f64, f64)>>;

/// Renders a soak run as a markdown table of per-metric slopes.
///
/// The slope is what distinguishes a leak from a one-off peak: a flat RSS with a
/// single early spike and a steadily climbing RSS can share a maximum, but only
/// the second one is a leak.
pub struct SoakReport {
    /// Samples keyed by metric name, each as `(elapsed minutes, value)`.
    pub samples: SoakSamples,
}

impl std::fmt::Display for SoakReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.samples.is_empty() {
            return write!(f, "(the workload printed no numeric metrics)");
        }
        writeln!(
            f,
            "| Metric | Samples | First | Last | Slope / min | Growth |"
        )?;
        write!(f, "|:---|---:|---:|---:|---:|---:|")?;
        for (metric, samples) in &self.samples {
            let (Some((_, first)), Some((_, last))) = (samples.first(), samples.last()) else {
                continue;
            };
            let slope = linear_slope(samples);
            let growth = if *first == 0.0 {
                "n/a".to_owned()
            } else {
                format!("{:+.1}%", (last / first - 1.) * 100.)
            };
            write!(
                f,
                "\n| {metric} | {} | {first:.0} | {last:.0} | {} | {growth} |",
                samples.len(),
                slope.map_or_else(|| "n/a".to_owned(), |slope| format!("{slope:+.2}")),
            )?;
        }
        Ok(())
    }
}

impl std::fmt::Display for PerfReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.deltas.is_empty() {
            return write!(f, "(no matching tests)");
        }
        let sorted = self.deltas.iter().collect::<Vec<_>>();
        writeln!(f, "| Category | Max | Mean | Min | Counted | Suppressed |")?;
        // We don't want to print too many newlines at the end, so handle newlines
        // a little jankily like this.
        write!(f, "|:---|---:|---:|---:|---:|---:|")?;
        for (cat, delta) in sorted.into_iter().rev() {
            const SIGN_POS: &str = "↑";
            const SIGN_NEG: &str = "↓";
            const SIGN_NEUTRAL_POS: &str = "±↑";
            const SIGN_NEUTRAL_NEG: &str = "±↓";

            let prettify = |time: f64| {
                let sign = if time > 0.05 {
                    SIGN_POS
                } else if time > 0. {
                    SIGN_NEUTRAL_POS
                } else if time > -0.05 {
                    SIGN_NEUTRAL_NEG
                } else {
                    SIGN_NEG
                };
                format!("{} {:.1}%", sign, time.abs() * 100.)
            };

            // Pretty-print these instead of just using the float display impl.
            write!(
                f,
                "\n| {cat} | {} | {} | {} | {} | {} |",
                prettify(delta.max),
                prettify(delta.mean),
                prettify(delta.min),
                delta.counted,
                delta.suppressed
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Timings` with no spread, so the throughput depends only on `mean_millis`.
    fn timing(mean_millis: u64) -> Timings {
        Timings {
            mean: Duration::from_millis(mean_millis),
            stddev: Duration::from_millis(0),
        }
    }

    /// An `Output` holding one successful test with `mean_millis` per `iterations`.
    fn output_with(name: &str, mean_millis: u64, iterations: NonZero<usize>) -> Output {
        let mut output = Output::blank();
        output.success(
            name,
            TestMdata {
                version: consts::MDATA_VER,
                iterations: Some(iterations),
                importance: Importance::Average,
                weight: consts::WEIGHT_DEFAULT,
            },
            iterations,
            timing(mean_millis),
        );
        output
    }

    #[test]
    fn parse_metric_line_folds_workload_into_the_metric_name() {
        let metrics = parse_metric_line(
            "EDITOR_FRAME_WORKLOAD workload=cached frames=10 allocations=512 bytes_per_frame=1.5",
        );

        assert_eq!(
            metrics,
            vec![
                ("EDITOR_FRAME_WORKLOAD[cached].frames".to_owned(), 10.0),
                (
                    "EDITOR_FRAME_WORKLOAD[cached].allocations".to_owned(),
                    512.0
                ),
                (
                    "EDITOR_FRAME_WORKLOAD[cached].bytes_per_frame".to_owned(),
                    1.5
                ),
            ]
        );
    }

    #[test]
    fn parse_metric_line_ignores_untagged_and_non_numeric_fields() {
        let untagged = parse_metric_line("frames=10");
        assert_eq!(untagged.len(), 0);
        assert_eq!(
            parse_metric_line("EDITOR_FRAME_RSS peak_kib=2048 note=unavailable"),
            vec![("EDITOR_FRAME_RSS.peak_kib".to_owned(), 2048.0)]
        );
    }

    #[test]
    fn linear_slope_recovers_a_known_gradient() {
        let samples = [(0.0, 100.0), (1.0, 110.0), (2.0, 120.0)];

        assert_eq!(linear_slope(&samples), Some(10.0));
        assert_eq!(linear_slope(&[(0.0, 1.0)]), None);
        assert_eq!(linear_slope(&[(1.0, 1.0), (1.0, 2.0)]), None);
    }

    #[test]
    fn noise_floor_records_the_measured_spread_per_test() {
        let first = output_with("test_a", 100, NonZero::new(10).unwrap());
        let second = output_with("test_a", 110, NonZero::new(10).unwrap());

        let floor = NoiseFloor::measure(&first, &second);

        // 10 iterations per 100 ms is 100/s against 90.9/s, i.e. a tenth apart.
        let spread = floor.spread_for("test_a");
        assert!((spread - 0.1).abs() < 1e-9, "{spread}");
        assert_eq!(floor.spread_for("unmeasured"), 0.0);
        assert!(!floor.is_empty());
    }

    #[test]
    fn compare_perf_suppresses_changes_inside_the_noise_floor() {
        let new = output_with("noisy", 103, NonZero::new(10).unwrap());
        let old = output_with("noisy", 100, NonZero::new(10).unwrap());
        let floor = NoiseFloor::measure(
            &output_with("noisy", 100, NonZero::new(10).unwrap()),
            &output_with("noisy", 104, NonZero::new(10).unwrap()),
        );

        let ungated = new.clone().compare_perf(old.clone(), None, 0.0).to_string();
        let gated = new
            .compare_perf(old, Some(&floor), DEFAULT_SIGNIFICANCE_THRESHOLD)
            .to_string();

        assert!(ungated.contains("3.0%"), "{ungated}");
        assert!(ungated.contains("| 1 | 0 |"), "{ungated}");
        assert!(gated.contains("0.0%"), "{gated}");
        assert!(gated.contains("| 0 | 1 |"), "{gated}");
    }

    #[test]
    fn compare_perf_counts_a_change_past_the_threshold() {
        let new = output_with("real", 120, NonZero::new(10).unwrap());
        let old = output_with("real", 100, NonZero::new(10).unwrap());

        let report = new
            .compare_perf(old, None, DEFAULT_SIGNIFICANCE_THRESHOLD)
            .to_string();

        assert!(report.contains("20.0%"), "{report}");
        assert!(report.contains("| 1 | 0 |"), "{report}");
    }
}
