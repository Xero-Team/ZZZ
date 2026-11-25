//! Opt-in conformance harness for the SVG renderer, driven by two reference
//! suites:
//!
//! - **`resvg/tests`** — the suite `usvg`/`resvg` is developed against, at the
//!   exact version this workspace pins (`0.50.0`), with reference PNGs produced
//!   by that version at 300px width. Because both sides come from the same
//!   engine build, agreement here is expected to be pixel-exact, which makes any
//!   disagreement a real finding rather than an antialiasing accident.
//! - **`resvg-test-suite`** — the cross-renderer suite with reference PNGs at
//!   500px width rendered by other engines. Agreement is measured as a visual
//!   closeness heuristic instead of exact equality, because a different
//!   rasterizer legitimately produces different edge pixels.
//!
//! Each case is rendered twice, and the pair is what makes a failure
//! attributable:
//!
//! 1. **Engine** — `usvg`/`resvg` driven the way the suite's own harness drives
//!    them (suite fonts, `resources_dir` set). Answers "does our pinned engine
//!    reproduce the reference at all", with no ZZZ code involved.
//! 2. **Shipped** — [`gpui::SvgRenderer`], the pipeline that actually rasterizes
//!    markdown-embedded SVG, the SVG preview, Mermaid output, and Typst
//!    previews. It resolves fonts from the system plus bundled fonts, refuses a
//!    `resources_dir`, caps the pixmap, and hands back demultiplied BGRA.
//!
//! A case failing layer 1 is engine/reference drift, not something this
//! workspace changed. A case passing layer 1 but failing layer 2 is a
//! difference in the shipped configuration, classified below as text/font
//! resolution, external resource containment, or unexplained.
//!
//! Layer 2 is also compared against layer 1 directly, with no reference PNG in
//! the way. Where those two rasters agree, the shipped configuration changed
//! nothing and the reference comparison is a statement about the engine alone;
//! where they disagree without a text or resource cause, the report says so and
//! the test fails, because that would be this workspace's own doing.
//!
//! The suites are third-party data under their own licenses and are not
//! vendored: `script/svg-spec` fetches them into `target/svg-spec/` (or
//! `$ZZZ_SVG_SPEC_DIR`). Without fixtures the test prints a skip notice and
//! passes, so offline CI is unaffected.
//!
//! The pass-count constants are a ratchet: they record the measured baseline, so
//! a regression fails while an improvement only changes the printed report until
//! someone raises the constant.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use gpui::{DevicePixels, RenderImage, SvgRenderer, SvgSize};

/// The width `resvg/tests` renders its reference PNGs at.
const RESVG_WIDTH: u32 = 300;
/// The width `resvg-test-suite` renders its reference PNGs at.
const CROSS_RENDERER_WIDTH: u32 = 500;

/// The exact-comparison tolerance, in 8-bit channels, matching the suite's own
/// harness.
const EXACT_THRESHOLD: u8 = 1;
/// The visual-comparison tolerance. A case counts as visually close when at
/// most this share of pixels differ by more than [`CLOSE_THRESHOLD`].
const CLOSE_THRESHOLD: u8 = 8;
const CLOSE_PIXEL_SHARE: u64 = 100;

/// Cases the suites' own harnesses skip: invalid documents, a filter that times
/// out on CI, and a SIMD rounding difference. Listed once because both suites
/// carry the same cases.
const SKIPPED_CASES: &[&str] = &[
    "filters/feMorphology/huge-radius",
    "structure/svg/negative-size",
    "structure/svg/no-size",
    "structure/svg/zero-size",
    "structure/svg/not-UTF-8-encoding",
    "paint-servers/radialGradient/focal-point-correction",
];

// Measured baselines live here; see the module docs for how to update them.
//
// These are floors, not exact expectations: the engine count can move by a few
// cases with the host's CPU (SIMD rounding) and the shipped count moves with the
// installed fonts, because the shipped configuration resolves fonts from the
// system. The printed report carries the exact numbers for the host that ran it.
const RESVG_ENGINE_MIN_PASSED: usize = 1780;
const RESVG_SHIPPED_MIN_PASSED: usize = 1330;
const CROSS_ENGINE_MIN_CLOSE: usize = 880;
const CROSS_SHIPPED_MIN_CLOSE: usize = 860;

#[test]
fn resvg_reference_suite() -> Result<()> {
    let Some(suite) = fixtures().map(|fixtures| fixtures.resvg) else {
        print_skip_notice();
        return Ok(());
    };

    let report = run_suite(&suite, &Policy::Exact)?;
    report.print("resvg/tests (pixel-exact, 300px)");
    report.assert_ratchet(RESVG_ENGINE_MIN_PASSED, RESVG_SHIPPED_MIN_PASSED)?;
    Ok(())
}

#[test]
fn cross_renderer_suite() -> Result<()> {
    let Some(suite) = fixtures().map(|fixtures| fixtures.cross_renderer) else {
        print_skip_notice();
        return Ok(());
    };

    let report = run_suite(&suite, &Policy::Close)?;
    report.print("resvg-test-suite (visual, 500px)");
    report.assert_ratchet(CROSS_ENGINE_MIN_CLOSE, CROSS_SHIPPED_MIN_CLOSE)?;
    Ok(())
}

fn print_skip_notice() {
    eprintln!(
        "skipped: SVG reference suites not present, run `script/svg-spec` \
         (or set ZZZ_SVG_SPEC_DIR)"
    );
}

/// Where the two suites live inside the fixture root. Each entry is missing when
/// that suite was not fetched, and the corresponding test then skips.
struct Fixtures {
    resvg: Suite,
    cross_renderer: Suite,
}

fn fixtures() -> Option<Fixtures> {
    let root = PathBuf::from(std::env::var_os("ZZZ_SVG_SPEC_DIR")?);

    // `resvg` keeps `tests/` (300px) and `extra/` (intrinsic size) side by side.
    let resvg_root = root.join("resvg");
    let resvg = Suite {
        fontdb: suite_fontdb(&resvg_root),
        cases: [
            collect_cases(&resvg_root.join("tests"), Target::ScaleToWidth(RESVG_WIDTH))
                .unwrap_or_default(),
            collect_cases(&resvg_root.join("extra"), Target::Intrinsic)
                .unwrap_or_default()
                .into_iter()
                .map(with_extra_override)
                .collect(),
        ]
        .into_iter()
        .flatten()
        .collect(),
    };

    let cross_root = root.join("cross-renderer");
    let cross_renderer = Suite {
        fontdb: suite_fontdb(&cross_root),
        cases: collect_cases(
            &cross_root.join("tests"),
            Target::ScaleToWidth(CROSS_RENDERER_WIDTH),
        )
        .unwrap_or_default(),
    };

    if resvg.cases.is_empty() && cross_renderer.cases.is_empty() {
        return None;
    }
    Some(Fixtures {
        resvg,
        cross_renderer,
    })
}

struct Suite {
    cases: Vec<Case>,
    /// The suite's own fonts. Each suite ships a different set, so this must be
    /// loaded per suite rather than cached process-wide.
    fontdb: Option<Arc<usvg::fontdb::Database>>,
}

struct Case {
    /// Suite-relative name, used in the report.
    name: String,
    svg_path: PathBuf,
    png_path: PathBuf,
    target: Target,
    /// `resvg`'s `extra` suite rasterizes a single element's bounding box for a
    /// few cases, which the whole-document pipeline cannot reproduce by design.
    /// Those cases only measure the engine.
    engine_only: bool,
}

#[derive(Clone, Copy)]
enum Target {
    /// Scale so the raster is exactly this many pixels wide, preserving aspect
    /// ratio. This is how both suites produce their reference PNGs.
    ScaleToWidth(u32),
    /// Rasterize at the size the document declares.
    Intrinsic,
    /// Rasterize at the declared size scaled by this factor. `extra` uses it for
    /// one case that is rendered at 10x.
    ScaleBy(f32),
    /// Rasterize one element's absolute bounding box, ignoring the viewport.
    /// `extra` uses it to test filters on a group in isolation.
    Node(&'static str),
}

/// `resvg`'s `extra/` cases are not all rendered at their intrinsic size; a few
/// narrow the raster to one element or scale it up. The overrides mirror
/// `crates/resvg/tests/integration/extra.rs`, which is the list that produced
/// those reference PNGs.
fn with_extra_override(mut case: Case) -> Case {
    let short = case
        .name
        .strip_prefix("extra/")
        .unwrap_or(&case.name)
        .to_owned();
    case.target = match short.as_str() {
        "filter-region-precision" => Target::ScaleBy(10.0),
        "filter-on-empty-group" => Target::Node("g1"),
        "filter-with-transform-on-shape" => Target::Node("g1"),
        _ => case.target,
    };
    case.engine_only = matches!(case.target, Target::Node(_));
    case
}

/// How strictly a rendered raster is compared against its reference.
enum Policy {
    /// Pixel equality within [`EXACT_THRESHOLD`]. Used where both sides come
    /// from the same engine build.
    Exact,
    /// Visual closeness. Used where the reference came from another renderer.
    ///
    /// This is a heuristic, not a spec claim: a case that fails it may still be
    /// a defensible difference in edge coverage or hinting. It exists to catch
    /// structural disagreements (wrong geometry, missing paint, dropped text)
    /// that exact comparison against a foreign rasterizer cannot separate from
    /// antialiasing.
    Close,
}

fn collect_cases(dir: &Path, target: Target) -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    collect_cases_at(dir, dir, target, &mut cases)?;
    Ok(cases)
}

fn collect_cases_at(root: &Path, dir: &Path, target: Target, cases: &mut Vec<Case>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {dir:?}"))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_cases_at(root, &path, target, cases)?;
        } else if path.extension().is_some_and(|extension| extension == "svg") {
            let relative = path
                .strip_prefix(root)
                .expect("case path is under the suite root")
                .with_extension("");
            let name = relative.to_string_lossy().replace('\\', "/");
            if SKIPPED_CASES
                .iter()
                .any(|skipped| name.starts_with(skipped))
            {
                continue;
            }
            let png_path = path.with_extension("png");
            if !png_path.exists() {
                continue;
            }
            cases.push(Case {
                name,
                svg_path: path,
                png_path,
                target,
                engine_only: false,
            });
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Report {
    total: usize,
    /// Cases that only the engine layer can reproduce.
    engine_only: usize,
    engine_passed: usize,
    shipped_passed: usize,
    /// Cases the shipped renderer reproduces byte-for-byte from the engine pass,
    /// i.e. where the shipped configuration changed nothing.
    shipped_matches_engine: usize,
    /// Layer 1 failures: engine/reference drift.
    engine_failures: Vec<String>,
    /// Cases where the shipped raster differs from the engine raster without a
    /// text or external-resource cause. This is the list that has to stay empty:
    /// it is the only bucket that accuses this workspace's own code.
    config_deviations: Vec<String>,
    /// Layer 2 failures, bucketed by why they are expected or not.
    shipped_failures: BTreeMap<FailureKind, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FailureKind {
    /// The engine already disagreed, so layer 2 is not evidence of anything.
    EngineDrift,
    /// The case draws text, and the shipped renderer resolves fonts from the
    /// system and bundled assets instead of the suite's font directory.
    TextFonts,
    /// The case references a sibling resource. The shipped renderer runs with
    /// `resources_dir: None`, so this is a containment decision, not a gap.
    ExternalResource,
    /// Neither of the above: a disagreement the suite calls a rendering bug.
    Unexplained,
}

impl FailureKind {
    fn label(self) -> &'static str {
        match self {
            Self::EngineDrift => "engine drift",
            Self::TextFonts => "text/font resolution",
            Self::ExternalResource => "external resource containment",
            Self::Unexplained => "UNEXPLAINED",
        }
    }
}

impl Report {
    fn print(&self, title: &str) {
        let compared = self.total - self.engine_only;
        println!("\n{title}");
        println!(
            "  cases    : {} total, {} with a shipped raster ({} engine-only)",
            self.total, compared, self.engine_only
        );
        println!(
            "  engine   : {} passed (usvg/resvg 0.50, suite fonts, resources_dir set)",
            self.engine_passed
        );
        println!(
            "  shipped  : {} passed (gpui::SvgRenderer, system fonts, no resources_dir)",
            self.shipped_passed
        );
        println!(
            "  config   : {}/{} render identically through engine and shipped",
            self.shipped_matches_engine, compared
        );

        if !self.config_deviations.is_empty() {
            println!(
                "\nDEVIATIONS between shipped and engine without a text/resource cause ({}):",
                self.config_deviations.len()
            );
            for case in &self.config_deviations {
                println!("  {case}");
            }
        }

        if !self.engine_failures.is_empty() {
            println!("\nengine drift ({}):", self.engine_failures.len());
            for case in &self.engine_failures {
                println!("  {case}");
            }
        }

        for (kind, cases) in &self.shipped_failures {
            let marker = if *kind == FailureKind::Unexplained {
                "UNEXPLAINED "
            } else {
                ""
            };
            println!(
                "\n{marker}shipped failures — {} ({}):",
                kind.label(),
                cases.len()
            );
            for case in cases {
                println!("  {case}");
            }
        }
    }

    fn assert_ratchet(&self, engine_min: usize, shipped_min: usize) -> Result<()> {
        anyhow::ensure!(
            self.engine_passed >= engine_min,
            "engine pass count regressed: {} < {}",
            self.engine_passed,
            engine_min
        );
        anyhow::ensure!(
            self.shipped_passed >= shipped_min,
            "shipped pass count regressed: {} < {}",
            self.shipped_passed,
            shipped_min
        );
        anyhow::ensure!(
            self.config_deviations.is_empty(),
            "the shipped renderer disagreed with the engine on cases that draw \
             neither text nor external resources:\n  {}",
            self.config_deviations.join("\n  ")
        );
        Ok(())
    }
}

fn run_suite(suite: &Suite, policy: &Policy) -> Result<Report> {
    let renderer = SvgRenderer::new(Arc::new(()));
    let fontdb = suite.fontdb.clone();
    let mut report = Report::default();
    let mut cases = suite.cases.iter().collect::<Vec<_>>();
    cases.sort_by(|left, right| left.name.cmp(&right.name));

    for case in cases {
        let svg =
            std::fs::read(&case.svg_path).with_context(|| format!("read {:?}", case.svg_path))?;
        let reference = load_reference(&case.png_path)?;
        report.total += 1;

        let engine = engine_render(&svg, &case.svg_path, &fontdb, case.target);
        // The shipped renderer runs even when the engine pass failed, so a case
        // the engine cannot reproduce is still evidence about the shipped path.
        let size = match &engine {
            Ok(rendered) => rendered.size,
            Err(_) => reference.size,
        };
        let shipped = if case.engine_only {
            Err(anyhow::anyhow!("engine-only case"))
        } else {
            shipped_render(&renderer, &svg, size)
        };
        debug_case(&case.name, &reference, &engine, &shipped);

        let engine_ok = engine
            .as_ref()
            .is_ok_and(|rendered| compare(rendered, &reference, policy));

        if engine_ok {
            report.engine_passed += 1;
        } else {
            report.engine_failures.push(case.name.clone());
        }

        if case.engine_only {
            report.engine_only += 1;
            continue;
        }

        let shipped_ok = shipped
            .as_ref()
            .is_ok_and(|rendered| compare(rendered, &reference, policy));

        if shipped_ok {
            report.shipped_passed += 1;
        } else {
            let kind = if !engine_ok {
                FailureKind::EngineDrift
            } else if references_external_resource(&svg) {
                FailureKind::ExternalResource
            } else if draws_text(&svg) {
                FailureKind::TextFonts
            } else {
                FailureKind::Unexplained
            };
            report
                .shipped_failures
                .entry(kind)
                .or_default()
                .push(case.name.clone());
        }

        // Layer 3: shipped against engine, with no reference in the way. Where
        // this agrees, the shipped configuration changed nothing and the
        // reference comparison above is a statement about the engine alone.
        let config_match = match (&engine, &shipped) {
            (Ok(engine), Ok(shipped)) => {
                engine.size == shipped.size && diff(engine, shipped).over_exact == 0
            }
            _ => false,
        };
        if config_match {
            report.shipped_matches_engine += 1;
        } else if !draws_text(&svg) && !references_external_resource(&svg) {
            report.config_deviations.push(case.name.clone());
        }
    }

    Ok(report)
}

/// A raster in RGBA order with straight (demultiplied) alpha, ready to compare
/// against a reference PNG.
struct Raster {
    size: (u32, u32),
    pixels: Vec<u8>,
}

fn load_reference(path: &Path) -> Result<Raster> {
    let image = image::open(path)
        .with_context(|| format!("decode reference PNG {path:?}"))?
        .to_rgba8();
    Ok(Raster {
        size: image.dimensions(),
        pixels: image.into_raw(),
    })
}

/// The suite's font database and generic-family mapping, mirroring
/// `resvg/crates/resvg/tests/integration/main.rs`.
fn suite_fontdb(root: &Path) -> Option<Arc<usvg::fontdb::Database>> {
    let fonts_dir = root.join("fonts");
    if !fonts_dir.is_dir() {
        return None;
    }
    Some(Arc::new({
        let mut paths: Vec<_> = std::fs::read_dir(&fonts_dir)
            .expect("suite fonts directory")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension, "ttf" | "ttc" | "otf" | "otc"))
            })
            .collect();
        paths.sort();

        let mut db = usvg::fontdb::Database::new();
        for path in paths {
            if let Err(error) = db.load_font_file(&path) {
                eprintln!("failed to load suite font {path:?}: {error}");
            }
        }
        db.set_serif_family("Noto Serif");
        db.set_sans_serif_family("Noto Sans");
        db.set_cursive_family("Yellowtail");
        db.set_fantasy_family("Sedgwick Ave Display");
        db.set_monospace_family("Noto Mono");
        db
    }))
}

fn engine_render(
    svg: &[u8],
    svg_path: &Path,
    fontdb: &Option<Arc<usvg::fontdb::Database>>,
    target: Target,
) -> Result<Raster> {
    let options = usvg::Options {
        fontdb: fontdb.clone().unwrap_or_default(),
        resources_dir: svg_path.parent().map(Path::to_path_buf),
        // The references were regenerated with the resvg CLI, whose documented
        // invocation passes `--font-family 'Noto Sans'`. Leaving the default
        // ("Times New Roman") moves every text case whose elements do not name
        // a family, which would otherwise read as hundreds of engine failures.
        font_family: "Noto Sans".to_owned(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(svg, &options).map_err(|error| anyhow::anyhow!(error))?;

    let tree_size = tree.size();
    let size = match target {
        Target::Node(id) => {
            let node = tree
                .node_by_id(id)
                .ok_or_else(|| anyhow::anyhow!("missing node {id}"))?;
            let bounds = node
                .abs_layer_bounding_box()
                .ok_or_else(|| anyhow::anyhow!("node {id} has no bounds"))?;
            let size = bounds.size().to_int_size();
            let mut pixmap = allocate(size)?;
            resvg::render_node(
                node,
                resvg::tiny_skia::Transform::identity(),
                &mut pixmap.as_mut(),
            );
            return finish(pixmap);
        }
        Target::Intrinsic => tree_size.to_int_size(),
        Target::ScaleToWidth(width) => tree_size
            .to_int_size()
            .scale_to_width(width)
            .ok_or_else(|| anyhow::anyhow!("zero-sized viewport"))?,
        Target::ScaleBy(scale) => tree_size
            .to_int_size()
            .scale_by(scale)
            .ok_or_else(|| anyhow::anyhow!("zero-sized viewport"))?,
    };

    let transform = match target {
        Target::Intrinsic => resvg::tiny_skia::Transform::identity(),
        Target::ScaleBy(scale) => resvg::tiny_skia::Transform::from_scale(scale, scale),
        Target::ScaleToWidth(_) => resvg::tiny_skia::Transform::from_scale(
            size.width() as f32 / tree_size.width(),
            size.height() as f32 / tree_size.height(),
        ),
        Target::Node(_) => unreachable!("node targets return above"),
    };

    let mut pixmap = allocate(size)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    finish(pixmap)
}

fn allocate(size: resvg::tiny_skia::IntSize) -> Result<resvg::tiny_skia::Pixmap> {
    if size.width() == 0 || size.height() == 0 {
        return Err(anyhow::anyhow!("empty viewport"));
    }
    resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("pixmap allocation failed"))
}

fn finish(pixmap: resvg::tiny_skia::Pixmap) -> Result<Raster> {
    let size = (pixmap.width(), pixmap.height());
    let mut pixels = pixmap.take();
    demultiply_alpha(&mut pixels);
    Ok(Raster { size, pixels })
}

fn shipped_render(renderer: &SvgRenderer, svg: &[u8], size: (u32, u32)) -> Result<Raster> {
    let tree = renderer
        .parse_svg(svg)
        .map_err(|error| anyhow::anyhow!(error))?;
    let requested = gpui::size(DevicePixels(size.0 as i32), DevicePixels(size.1 as i32));
    let image: Arc<RenderImage> = renderer
        .render_parsed(&tree, SvgSize::ExactSize(requested))
        .map_err(|error| anyhow::anyhow!(error))?;

    let mut pixels = image
        .as_bytes(0)
        .ok_or_else(|| anyhow::anyhow!("no frame"))?
        .to_vec();
    // `SvgRenderer` hands back demultiplied BGRA; the reference is RGBA.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(Raster { size, pixels })
}

/// How far a raster is from its reference.
struct Diff {
    /// `false` when the two sides computed different viewport sizes.
    same_size: bool,
    /// Pixels off by more than [`EXACT_THRESHOLD`], relative to the number of
    /// really compared pixels (transparent-vs-transparent pairs are skipped).
    over_exact: u64,
    /// Pixels off by more than [`CLOSE_THRESHOLD`].
    over_close: u64,
    compared: u64,
}

/// Measures a raster against its reference. Sizes must agree exactly: the
/// reference was produced at the same computed size, so a size mismatch means
/// the two sides disagree about the viewport, which no pixel tolerance can
/// excuse.
fn diff(rendered: &Raster, reference: &Raster) -> Diff {
    if rendered.size != reference.size {
        return Diff {
            same_size: false,
            over_exact: u64::MAX,
            over_close: u64::MAX,
            compared: 0,
        };
    }

    let mut compared = 0_u64;
    let mut over_close = 0_u64;
    let mut over_exact = 0_u64;
    for (actual, expected) in rendered
        .pixels
        .chunks_exact(4)
        .zip(reference.pixels.chunks_exact(4))
    {
        // Two fully transparent pixels are equal regardless of their colour
        // channels, matching the suite's own comparison.
        if actual[3] == 0 && expected[3] == 0 {
            continue;
        }
        compared += 1;
        let deviation = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if deviation > EXACT_THRESHOLD {
            over_exact += 1;
        }
        if deviation > CLOSE_THRESHOLD {
            over_close += 1;
        }
    }

    Diff {
        same_size: true,
        over_exact,
        over_close,
        compared,
    }
}

fn compare(rendered: &Raster, reference: &Raster, policy: &Policy) -> bool {
    let diff = diff(rendered, reference);
    if !diff.same_size {
        return false;
    }
    match policy {
        Policy::Exact => diff.over_exact == 0,
        Policy::Close => diff.over_close * CLOSE_PIXEL_SHARE <= diff.compared.max(1),
    }
}

/// Prints why one case disagreed, for `ZZZ_SVG_SPEC_DEBUG=<case name>`.
fn debug_case(name: &str, reference: &Raster, engine: &Result<Raster>, shipped: &Result<Raster>) {
    let Some(wanted) = std::env::var_os("ZZZ_SVG_SPEC_DEBUG") else {
        return;
    };
    if wanted != name {
        return;
    }
    eprintln!(
        "{name}: reference {}x{}",
        reference.size.0, reference.size.1
    );
    for (label, rendered) in [("engine", engine), ("shipped", shipped)] {
        match rendered {
            Ok(rendered) => {
                let diff = diff(rendered, reference);
                eprintln!(
                    "  {label}: {}x{}, same_size={}, compared={}, over_exact={}, over_close={}",
                    rendered.size.0,
                    rendered.size.1,
                    diff.same_size,
                    diff.compared,
                    diff.over_exact,
                    diff.over_close
                );
            }
            Err(error) => eprintln!("  {label}: error: {error:#}"),
        }
    }
}

fn demultiply_alpha(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as f64 / 255.0;
        if alpha == 0.0 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = (*channel as f64 / alpha + 0.5) as u8;
        }
    }
}

/// True when the document draws text, which the shipped renderer resolves from
/// system and bundled fonts rather than from the suite's font directory.
///
/// This is a heuristic over the serialized document, not a parse. It also flags
/// documents that embed a sub-SVG as a data URL, because the embedded document
/// can draw text the outer one does not mention (`structure/image/embedded-svg-with-text`).
fn draws_text(svg: &[u8]) -> bool {
    let text = String::from_utf8_lossy(svg);
    text.contains("<text")
        || text.contains("<tspan")
        || text.contains("<textPath")
        || (text.contains("image/svg+xml") && text.contains("base64"))
}

/// True when the document pulls in a sibling file rather than a fragment or a
/// data URL.
fn references_external_resource(svg: &[u8]) -> bool {
    let text = String::from_utf8_lossy(svg);
    ["href=\"", "href='"]
        .iter()
        .flat_map(|prefix| text.match_indices(prefix))
        .map(|(offset, prefix)| {
            let rest = &text[offset + prefix.len()..];
            let end = rest.find(['"', '\'']).unwrap_or(0);
            &rest[..end]
        })
        .any(|value| !value.is_empty() && !value.starts_with('#') && !value.starts_with("data:"))
}
