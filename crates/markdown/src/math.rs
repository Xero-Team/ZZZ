//! LaTeX math rendering for Markdown surfaces, backed by the pure-Rust
//! [`ratex`](https://github.com/erweixin/RaTeX) engine.
//!
//! Math expressions are extracted from the markdown source independently of
//! pulldown-cmark's own math extension (which is deliberately disabled because
//! `$` conflicts with links and currency). Detected formulas are laid out by
//! `ratex-layout` and serialized to self-contained SVG with `ratex-svg`, then
//! rasterized through GPUI's SVG renderer and cached - mirroring the mermaid
//! pipeline in [`crate::mermaid`].

use collections::{HashMap, HashSet};
use gpui::{
    AnyElement, App, Context, Hsla, ImageSource, Pixels, RenderImage, Rgba, SharedString, Task, img,
};
use i18n::tr;
use std::ops::Range;
use std::sync::{Arc, OnceLock};
use ui::prelude::*;

use crate::Markdown;
use crate::parser::{MarkdownEvent, MarkdownTag};

/// Formulas longer than this are not rendered; this bounds the work a hostile
/// document can trigger (a very wide formula can otherwise allocate hundreds of
/// megabytes of SVG).
const MAX_FORMULA_BYTES: usize = 4096;

/// Maximum number of formulas rendered from one Markdown document. This keeps
/// a hostile document from spawning an unbounded number of rasterization tasks.
const MAX_MATH_EXPRESSIONS: usize = 512;

/// Inline math delimiters.
const INLINE_DELIMITER: u8 = b'$';
/// Display math delimiter.
const DISPLAY_DELIMITER: &[u8] = b"$$";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MarkdownMathKind {
    Inline,
    Display,
}

impl MarkdownMathKind {
    fn is_display(self) -> bool {
        matches!(self, Self::Display)
    }
}

/// The cache identity of a formula, independent of its position in the source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ParsedMarkdownMathContents {
    pub(crate) formula: SharedString,
    pub(crate) display: bool,
}

/// A math expression found in the markdown source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedMarkdownMath {
    /// Range covering the delimiters (or the whole fenced block).
    pub(crate) source_range: Range<usize>,
    /// Range of the formula body, excluding delimiters.
    pub(crate) content_range: Range<usize>,
    pub(crate) kind: MarkdownMathKind,
    /// True for ```` ```math ```` fenced blocks, which render as display math.
    pub(crate) fenced: bool,
    pub(crate) contents: ParsedMarkdownMathContents,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MathCacheKey {
    contents: ParsedMarkdownMathContents,
    font_size_bits: u32,
    color: u32,
}

impl MathCacheKey {
    fn new(contents: ParsedMarkdownMathContents, font_size: Pixels, color: Hsla) -> Self {
        Self {
            contents,
            font_size_bits: font_size.as_f32().to_bits(),
            color: pack_color(color),
        }
    }
}

struct CachedMath {
    render_image: Arc<OnceLock<anyhow::Result<Arc<RenderImage>>>>,
    _task: Task<()>,
}

#[derive(Default, Clone)]
pub(crate) struct MathState {
    entries: HashMap<MathCacheKey, Arc<CachedMath>>,
}

impl MathState {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Ensure every expression has an in-flight or completed render for the
    /// given font size and color, and evict entries for formulas that are no
    /// longer present.
    pub(crate) fn update(
        &mut self,
        expressions: &[ParsedMarkdownMath],
        font_size: Pixels,
        color: Hsla,
        cx: &mut Context<Markdown>,
    ) {
        let current: HashSet<MathCacheKey> = expressions
            .iter()
            .map(|math| MathCacheKey::new(math.contents.clone(), font_size, color))
            .collect();
        self.entries.retain(|key, _| current.contains(key));

        let svg_renderer = cx.svg_renderer();
        for math in expressions {
            let key = MathCacheKey::new(math.contents.clone(), font_size, color);
            if self.entries.contains_key(&key) {
                continue;
            }
            self.entries.insert(
                key,
                Arc::new(CachedMath::new(
                    math.contents.clone(),
                    font_size,
                    color,
                    svg_renderer.clone(),
                    cx,
                )),
            );
        }
    }

    fn get(&self, key: &MathCacheKey) -> Option<&Arc<CachedMath>> {
        self.entries.get(key)
    }
}

impl CachedMath {
    fn new(
        contents: ParsedMarkdownMathContents,
        font_size: Pixels,
        color: Hsla,
        svg_renderer: gpui::SvgRenderer,
        cx: &mut Context<Markdown>,
    ) -> Self {
        let render_image = Arc::new(OnceLock::<anyhow::Result<Arc<RenderImage>>>::new());
        let render_image_clone = render_image.clone();

        let task = cx.spawn(async move |this, cx| {
            let value = cx
                .background_spawn(async move {
                    render_math_image(&contents, font_size, color, svg_renderer)
                })
                .await;
            if render_image_clone.set(value).is_err() {
                log::debug!("math render result was already initialized");
            }
            if let Some(this) = this.upgrade() {
                this.update(cx, |_, cx| cx.notify());
            }
        });

        Self {
            render_image,
            _task: task,
        }
    }
}

fn pack_color(color: Hsla) -> u32 {
    let rgba: Rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(rgba.r) << 24) | (channel(rgba.g) << 16) | (channel(rgba.b) << 8) | channel(rgba.a)
}

/// Parse, lay out, serialize and rasterize a single formula.
///
/// `ratex` is written for untrusted input, but we still defend against panics
/// and oversized inputs so that a malformed formula can never take down the
/// preview.
fn render_math_image(
    contents: &ParsedMarkdownMathContents,
    font_size: Pixels,
    color: Hsla,
    svg_renderer: gpui::SvgRenderer,
) -> anyhow::Result<Arc<RenderImage>> {
    let formula = contents.formula.as_ref();
    anyhow::ensure!(
        formula.len() <= MAX_FORMULA_BYTES,
        "formula is too long ({} bytes)",
        formula.len()
    );
    anyhow::ensure!(!formula.trim().is_empty(), "empty formula");

    let svg = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_formula_to_svg(formula, contents.display, font_size, color)
    }))
    .map_err(|_| anyhow::anyhow!("math renderer panicked"))??;

    svg_renderer
        .render_single_frame(svg.as_bytes(), 1.0)
        .map_err(|error| anyhow::anyhow!("{error}"))
}

fn render_formula_to_svg(
    formula: &str,
    display: bool,
    font_size: Pixels,
    color: Hsla,
) -> anyhow::Result<String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list};
    use ratex_types::color::Color;
    use ratex_types::math_style::MathStyle;

    let ast = ratex_parser::parser::parse(formula).map_err(|error| anyhow::anyhow!("{error}"))?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let rgba: Rgba = color.to_rgb();
    let layout_options = LayoutOptions::default()
        .with_style(style)
        .with_color(Color::new(rgba.r, rgba.g, rgba.b, rgba.a));
    let layout_box = layout(&ast, &layout_options);
    let display_list = to_display_list(&layout_box);
    Ok(ratex_svg::render_to_svg(
        &display_list,
        &ratex_svg::SvgOptions {
            font_size: font_size.as_f32() as f64,
            padding: 0.0,
            embed_glyphs: true,
            ..Default::default()
        },
    ))
}

/// Find the math expression to render for `paragraph`, if the paragraph is
/// nothing but a single display formula (possibly spanning multiple lines).
pub(crate) fn block_math_for_paragraph<'a>(
    source: &str,
    expressions: &'a [ParsedMarkdownMath],
    paragraph: &Range<usize>,
) -> Option<&'a ParsedMarkdownMath> {
    let start = expressions.partition_point(|math| math.source_range.start < paragraph.start);
    let end = expressions.partition_point(|math| math.source_range.start < paragraph.end);
    expressions[start..end].iter().find(|math| {
        !math.fenced
            && math.kind.is_display()
            && !math.contents.formula.trim().is_empty()
            && math.source_range.start >= paragraph.start
            && math.source_range.end <= paragraph.end
            && source[paragraph.start..math.source_range.start]
                .trim()
                .is_empty()
            && source[math.source_range.end..paragraph.end]
                .trim()
                .is_empty()
    })
}

/// Find the fenced math block starting exactly at `start`, if any.
pub(crate) fn fenced_math_at(
    expressions: &[ParsedMarkdownMath],
    start: usize,
) -> Option<&ParsedMarkdownMath> {
    let index = expressions.partition_point(|math| math.source_range.start < start);
    expressions
        .get(index)
        .filter(|math| math.fenced && math.source_range.start == start)
}

/// The slice of expressions whose start falls within `range`.
pub(crate) fn math_for_text_range<'a>(
    expressions: &'a [ParsedMarkdownMath],
    range: &Range<usize>,
) -> &'a [ParsedMarkdownMath] {
    let start = expressions.partition_point(|math| math.source_range.start < range.start);
    let end = expressions.partition_point(|math| math.source_range.start < range.end);
    &expressions[start..end]
}

/// Build the element for a math expression, using the cached rasterized image
/// when available and falling back to the raw formula otherwise.
pub(crate) fn render_math(
    math: &ParsedMarkdownMath,
    state: &MathState,
    font_size: Pixels,
    color: Hsla,
    cx: &App,
) -> AnyElement {
    let key = MathCacheKey::new(math.contents.clone(), font_size, color);

    // Formulas that only define macros (or were otherwise reduced to nothing)
    // draw no output.
    if math.contents.formula.trim().is_empty() {
        return div().into_any_element();
    }

    let failed = tr(
        cx,
        "markdown.math.failed_to_render",
        "Failed to render equation",
    );

    match state.get(&key).and_then(|cached| cached.render_image.get()) {
        Some(Ok(image)) => {
            let image = image.clone();
            img(ImageSource::Render(image))
                .max_w_full()
                .with_fallback(move || {
                    div()
                        .child(Label::new(failed.clone()).color(Color::Muted))
                        .into_any_element()
                })
                .into_any_element()
        }
        Some(Err(_)) => fallback_formula(math),
        None => {
            let rendering = tr(cx, "markdown.math.rendering", "Rendering equation...");
            Label::new(rendering).color(Color::Muted).into_any_element()
        }
    }
}

fn fallback_formula(math: &ParsedMarkdownMath) -> AnyElement {
    let formula = math.contents.formula.clone();
    let text = if math.kind.is_display() {
        SharedString::from(format!("$${formula}$$"))
    } else {
        SharedString::from(format!("${formula}$"))
    };
    div()
        .child(Label::new(text).color(Color::Muted))
        .into_any_element()
}

/// Record a formula unless its trimmed body is empty.
fn push_expression(
    expressions: &mut Vec<ParsedMarkdownMath>,
    source: &str,
    content_range: Range<usize>,
    source_range: Range<usize>,
    kind: MarkdownMathKind,
    fenced: bool,
) {
    let contents = trimmed_contents(source, content_range.clone());
    if contents.trim().is_empty() {
        return;
    }
    expressions.push(ParsedMarkdownMath {
        source_range,
        content_range,
        kind,
        fenced,
        contents: ParsedMarkdownMathContents {
            formula: contents,
            display: kind.is_display(),
        },
    });
}

/// Extract math expressions from the markdown source.
///
/// `$...$`, `$$...$$`, `\(...\)`, `\[...\]` and ```` ```math ```` /
/// ```` ```latex ```` fenced blocks are recognized. Code spans, code blocks,
/// HTML tags and metadata blocks are skipped.
pub(crate) fn extract_math_expressions(
    source: &str,
    events: &[(Range<usize>, MarkdownEvent)],
) -> Vec<ParsedMarkdownMath> {
    let mut fenced_expressions = Vec::new();
    let excluded = excluded_ranges(events);

    for (range, event) in events {
        if fenced_expressions.len() == MAX_MATH_EXPRESSIONS {
            break;
        }
        let MarkdownEvent::Start(MarkdownTag::CodeBlock { kind, metadata }) = event else {
            continue;
        };
        let crate::parser::CodeBlockKind::FencedLang(info) = kind else {
            continue;
        };
        if !is_math_fence(info.as_ref()) {
            continue;
        }
        push_expression(
            &mut fenced_expressions,
            source,
            metadata.content_range.clone(),
            range.clone(),
            MarkdownMathKind::Display,
            true,
        );
    }

    let bytes = source.as_bytes();
    let mut inline_expressions = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() && inline_expressions.len() < MAX_MATH_EXPRESSIONS {
        if let Some(end) = exclusion_end(&excluded, cursor) {
            cursor = end.max(cursor + 1);
            continue;
        }
        match bytes[cursor] {
            b'\\' if bytes.get(cursor + 1) == Some(&b'(') => {
                if let Some((content_range, source_range)) =
                    scan_backslash_math(bytes, &excluded, cursor, b')', false)
                {
                    push_expression(
                        &mut inline_expressions,
                        source,
                        content_range,
                        source_range.clone(),
                        MarkdownMathKind::Inline,
                        false,
                    );
                    cursor = source_range.end;
                } else {
                    cursor += 2;
                }
            }
            b'\\' if bytes.get(cursor + 1) == Some(&b'[') => {
                if let Some((content_range, source_range)) =
                    scan_backslash_math(bytes, &excluded, cursor, b']', true)
                {
                    push_expression(
                        &mut inline_expressions,
                        source,
                        content_range,
                        source_range.clone(),
                        MarkdownMathKind::Display,
                        false,
                    );
                    cursor = source_range.end;
                } else {
                    cursor += 2;
                }
            }
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'$' if bytes.get(cursor + 1) == Some(&b'$') => {
                if let Some((content_range, source_range)) =
                    scan_display_math(bytes, &excluded, cursor)
                {
                    push_expression(
                        &mut inline_expressions,
                        source,
                        content_range,
                        source_range.clone(),
                        MarkdownMathKind::Display,
                        false,
                    );
                    cursor = source_range.end;
                } else {
                    cursor += 2;
                }
            }
            b'$' => {
                if let Some((content_range, source_range)) =
                    scan_inline_math(bytes, &excluded, cursor)
                {
                    push_expression(
                        &mut inline_expressions,
                        source,
                        content_range,
                        source_range.clone(),
                        MarkdownMathKind::Inline,
                        false,
                    );
                    cursor = source_range.end;
                } else {
                    cursor += 1;
                }
            }
            _ => cursor += 1,
        }
    }

    let mut expressions = fenced_expressions;
    expressions.extend(inline_expressions);
    expressions.sort_by_key(|math| math.source_range.start);
    expressions.truncate(MAX_MATH_EXPRESSIONS);
    prepare_formulas(&mut expressions);
    expressions
}

/// Make the extracted formulas renderable by `ratex`, which - unlike MathJax -
/// parses each formula in isolation:
///
/// * `\label{...}` is dropped and `\eqref{...}` / `\ref{...}` are turned into
///   plain text, since we don't track equation numbers.
/// * User macros defined with `\newcommand` / `\renewcommand` /
///   `\providecommand` in one formula are made available to the other formulas
///   that reference them, matching MathJax's page-level macro scope.
fn prepare_formulas(expressions: &mut [ParsedMarkdownMath]) {
    let mut collected: Vec<(String, String)> = Vec::new();
    for math in expressions.iter() {
        collect_macro_definitions(math.contents.formula.as_ref(), &mut collected);
    }
    // Keep one definition per macro name, preferring the last one seen.
    let mut definitions: Vec<(String, String)> = Vec::new();
    for (name, definition) in collected {
        if let Some(existing) = definitions.iter_mut().find(|(n, _)| *n == name) {
            existing.1 = definition;
        } else {
            definitions.push((name, definition));
        }
    }

    for math in expressions.iter_mut() {
        let formula = math.contents.formula.to_string();
        // Definitions are hoisted to the page level, so strip them from the
        // formula body that is actually rendered.
        let body = sanitize_equation_references(&strip_macro_definitions(&formula));
        let mut prefix = String::new();
        for (name, definition) in &definitions {
            if references_macro(&body, name) {
                prefix.push_str(definition);
                prefix.push('\n');
            }
        }
        let prepared = format!("{prefix}{body}");
        let prepared = prepared.trim();
        if prepared != formula {
            math.contents.formula = prepared.to_owned().into();
        }
    }
}

const MACRO_PREFIXES: [&str; 3] = ["\\newcommand", "\\renewcommand", "\\providecommand"];

fn collect_macro_definitions(source: &str, out: &mut Vec<(String, String)>) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            index += 1;
            continue;
        }
        let Some((name, definition, end)) = parse_macro_definition(source, index) else {
            index += 1;
            continue;
        };
        out.push((name, definition));
        index = end;
    }
}

/// Remove every `\newcommand`-family definition from `source`, leaving only the
/// expression that the formula actually draws.
fn strip_macro_definitions(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut result = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && let Some((_, _, end)) = parse_macro_definition(source, index)
        {
            index = end;
            continue;
        }
        let character = source[index..]
            .chars()
            .next()
            .expect("iterator should yield an item");
        result.push(character);
        index += character.len_utf8();
    }
    result
}

/// Parse a macro definition beginning at `start` (which must be the `\` of a
/// `\newcommand`-family command). Returns the macro name, the full definition
/// source and the index just past it.
fn parse_macro_definition(source: &str, start: usize) -> Option<(String, String, usize)> {
    let bytes = source.as_bytes();
    let prefix = MACRO_PREFIXES
        .iter()
        .find(|prefix| source[start..].starts_with(**prefix))?;
    let mut index = start + prefix.len();
    if bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }

    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }

    // The macro name is either `\name` or `{\name}`.
    let name;
    if bytes.get(index) == Some(&b'{') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'\\') {
            return None;
        }
        let (parsed, next) = parse_macro_name(source, index)?;
        name = parsed;
        index = next;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'}') {
            return None;
        }
        index += 1;
    } else if bytes.get(index) == Some(&b'\\') {
        let (parsed, next) = parse_macro_name(source, index)?;
        name = parsed;
        index = next;
    } else {
        return None;
    }

    // Optional `[argument count]` and `[default]` groups.
    for _ in 0..2 {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'[') {
            index = find_matching(source, index, b'[', b']')?;
        } else {
            break;
        }
    }

    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    let end = find_matching(source, index, b'{', b'}')?;
    Some((name, source[start..end].to_owned(), end))
}

fn parse_macro_name(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'\\') {
        return None;
    }
    let mut index = start + 1;
    let name_start = index;
    if bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
        while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
            index += 1;
        }
    } else {
        // A single non-letter control sequence, e.g. `\,`.
        index += 1;
    }
    Some((source[name_start..index].to_owned(), index))
}

/// `\label{...}` is removed; `\eqref{...}` and `\ref{...}` become `\text{...}`
/// so the missing equation number is at least visible.
fn sanitize_equation_references(formula: &str) -> String {
    let without_labels = replace_braced_command(formula, "\\label", |_| String::new());
    let with_eqrefs = replace_braced_command(&without_labels, "\\eqref", |name| {
        format!("\\text{{({name})}}")
    });
    replace_braced_command(&with_eqrefs, "\\ref", |name| format!("\\text{{{name}}}"))
}

fn references_macro(formula: &str, name: &str) -> bool {
    let needle = format!("\\{name}");
    let bytes = formula.as_bytes();
    let mut index = 0;
    while let Some(offset) = formula[index..].find(&needle) {
        let position = index + offset;
        let after = position + needle.len();
        if !bytes.get(after).is_some_and(u8::is_ascii_alphabetic) {
            return true;
        }
        index = position + 1;
    }
    false
}

/// Replace every `command{argument}` in `source` with `replacement(argument)`.
fn replace_braced_command(
    source: &str,
    command: &str,
    mut replacement: impl FnMut(&str) -> String,
) -> String {
    let mut result = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && source[index..].starts_with(command)
            && !bytes
                .get(index + command.len())
                .is_some_and(u8::is_ascii_alphabetic)
        {
            let mut open = index + command.len();
            while bytes.get(open).is_some_and(u8::is_ascii_whitespace) {
                open += 1;
            }
            if bytes.get(open) == Some(&b'{')
                && let Some(end) = find_matching(source, open, b'{', b'}')
            {
                result.push_str(&replacement(&source[open + 1..end - 1]));
                index = end;
                continue;
            }
        }
        let character = source[index..]
            .chars()
            .next()
            .expect("iterator should yield an item");
        result.push(character);
        index += character.len_utf8();
    }
    result
}

/// Given the index of an `open` delimiter, return the index just past its
/// matching `close`, honoring nesting and backslash escapes.
fn find_matching(source: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&open) {
        return None;
    }
    let mut depth = 0usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == open => {
                depth += 1;
                index += 1;
            }
            byte if byte == close => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// Rewrite `events` so that every non-fenced formula occupies a single
/// [`MarkdownEvent::Text`] spanning its whole [`source_range`](ParsedMarkdownMath::source_range).
///
/// Markdown inline syntax inside a formula (`*`, `_`, `[`, line breaks, ...)
/// would otherwise split the formula across several events - or even emit
/// emphasis/link tags for it - making it impossible to replace with a single
/// rendered image. Tags that lie fully inside a formula, or that cross a
/// formula boundary, are dropped so the remaining event stream stays balanced.
///
/// [`crate::Markdown`] runs this whenever `render_math` is enabled, after
/// extracting the formulas.
pub(crate) fn shield_math_events(
    events: &mut Vec<(Range<usize>, MarkdownEvent)>,
    expressions: &[ParsedMarkdownMath],
) {
    let spans: Vec<Range<usize>> = expressions
        .iter()
        .filter(|math| !math.fenced)
        .map(|math| math.source_range.clone())
        .collect();
    if spans.is_empty() {
        return;
    }

    // A tag that contains every formula it overlaps is preserved (e.g. a
    // paragraph, heading, link or emphasis that *wraps* a formula). Any other
    // tag is corrupted by the formula and is dropped, together with its pair.
    let keep_tag = |range: &Range<usize>| -> bool {
        spans
            .iter()
            .filter(|span| overlaps(range, span))
            .all(|span| range.start <= span.start && span.end <= range.end)
    };

    let mut result = Vec::with_capacity(events.len());
    let mut inserted: Vec<bool> = vec![false; spans.len()];

    for (range, event) in events.drain(..) {
        match &event {
            MarkdownEvent::Start(_)
            | MarkdownEvent::End(_)
            | MarkdownEvent::RootStart
            | MarkdownEvent::RootEnd(_) => {
                if keep_tag(&range) {
                    result.push((range, event));
                }
                continue;
            }
            _ => {}
        }

        // Walk the formulas overlapping this event, emitting the parts of the
        // event that fall outside them and one `Text` event per formula.
        let first = spans.partition_point(|span| span.end <= range.start);
        let mut cursor = range.start;
        let mut overlapped = false;
        for index in first..spans.len() {
            let span = &spans[index];
            if span.start >= range.end {
                break;
            }
            overlapped = true;
            if cursor < span.start {
                push_event_piece(&mut result, &event, cursor..span.start, &range);
            }
            if !inserted[index] {
                result.push((span.clone(), MarkdownEvent::Text));
                inserted[index] = true;
            }
            cursor = span.end;
        }
        if cursor < range.end {
            if overlapped {
                push_event_piece(&mut result, &event, cursor..range.end, &range);
            } else {
                result.push((range, event));
            }
        } else if !overlapped {
            result.push((range, event));
        }
    }

    *events = result;
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Emit the `piece` of `event`. Substituted text can't be split reliably, so
/// its pieces fall back to reading the original source as plain text.
fn push_event_piece(
    out: &mut Vec<(Range<usize>, MarkdownEvent)>,
    event: &MarkdownEvent,
    piece: Range<usize>,
    event_range: &Range<usize>,
) {
    match event {
        MarkdownEvent::SubstitutedText(_) | MarkdownEvent::SubstitutedCode(_)
            if &piece != event_range =>
        {
            out.push((piece, MarkdownEvent::Text));
        }
        _ => out.push((piece, event.clone())),
    }
}

fn trimmed_contents(source: &str, range: Range<usize>) -> SharedString {
    source[range].trim().to_owned().into()
}

fn is_math_fence(info: &str) -> bool {
    matches!(
        info.split_whitespace().next().unwrap_or_default(),
        "math" | "latex" | "tex" | "katex"
    )
}

fn scan_display_math(
    bytes: &[u8],
    excluded: &[Range<usize>],
    open: usize,
) -> Option<(Range<usize>, Range<usize>)> {
    let content_start = open + DISPLAY_DELIMITER.len();
    let mut cursor = content_start;
    while cursor + 1 < bytes.len() {
        if exclusion_end(excluded, cursor).is_some() {
            return None;
        }
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'$' if bytes.get(cursor + 1) == Some(&b'$') => {
                return Some((content_start..cursor, open..cursor + 2));
            }
            _ => cursor += 1,
        }
    }
    None
}

fn scan_inline_math(
    bytes: &[u8],
    excluded: &[Range<usize>],
    open: usize,
) -> Option<(Range<usize>, Range<usize>)> {
    let content_start = open + 1;
    let first = *bytes.get(content_start)?;
    // Opening delimiter must be immediately followed by non-whitespace, which
    // keeps currency like "$5" from opening a formula.
    if first.is_ascii_whitespace() || first == INLINE_DELIMITER {
        return None;
    }

    let mut cursor = content_start;
    while cursor < bytes.len() {
        if exclusion_end(excluded, cursor).is_some() {
            return None;
        }
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            // Inline math never spans a line break; multi-line formulas should
            // use `$$...$$` (handled as a block) or a fenced block.
            b'\n' => return None,
            b'$' => {
                let previous = bytes[cursor - 1];
                let after = bytes.get(cursor + 1);
                if !previous.is_ascii_whitespace()
                    && !after.is_some_and(u8::is_ascii_digit)
                    && cursor > content_start
                {
                    return Some((content_start..cursor, open..cursor + 1));
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn scan_backslash_math(
    bytes: &[u8],
    excluded: &[Range<usize>],
    open: usize,
    close: u8,
    allow_newlines: bool,
) -> Option<(Range<usize>, Range<usize>)> {
    let content_start = open + 2;
    let mut cursor = content_start;
    while cursor + 1 < bytes.len() {
        if exclusion_end(excluded, cursor).is_some() {
            return None;
        }
        match bytes[cursor] {
            b'\n' if !allow_newlines => return None,
            b'\\' if bytes.get(cursor + 1) == Some(&close) => {
                return Some((content_start..cursor, open..cursor + 2));
            }
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            _ => cursor += 1,
        }
    }
    None
}

fn excluded_ranges(events: &[(Range<usize>, MarkdownEvent)]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for (range, event) in events {
        match event {
            MarkdownEvent::Code | MarkdownEvent::SubstitutedCode(_) => ranges.push(range.clone()),
            MarkdownEvent::Html | MarkdownEvent::InlineHtml => ranges.push(range.clone()),
            MarkdownEvent::Start(MarkdownTag::CodeBlock { .. } | MarkdownTag::MetadataBlock(_)) => {
                ranges.push(range.clone())
            }
            _ => {}
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// If `position` lies inside an excluded range, returns that range's end.
fn exclusion_end(excluded: &[Range<usize>], position: usize) -> Option<usize> {
    let index = excluded.partition_point(|range| range.end <= position);
    excluded
        .get(index)
        .filter(|range| range.start <= position)
        .map(|range| range.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{MarkdownTagEnd, parse_markdown_with_options};
    use crate::{MarkdownElement, MarkdownOptions, MarkdownStyle};
    use gpui::{IntoElement, Render, TestAppContext, Window, size};

    fn ensure_theme_initialized(cx: &mut TestAppContext) {
        cx.update(|cx| {
            if !cx.has_global::<settings::SettingsStore>() {
                settings::init(cx);
            }
            i18n::init(cx);
            if !cx.has_global::<theme::GlobalTheme>() {
                theme_settings::init(theme::LoadThemes::JustBase, cx);
            }
        });
    }

    #[gpui::test]
    fn renders_math_formula_to_image(cx: &mut TestAppContext) {
        struct TestWindow;

        impl Render for TestWindow {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }

        ensure_theme_initialized(cx);
        let (_, cx) = cx.add_window_view(|_, _| TestWindow);
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                "$E = mc^2$".into(),
                None,
                None,
                MarkdownOptions {
                    render_math: true,
                    ..Default::default()
                },
                cx,
            )
        });
        cx.run_until_parked();
        cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| MarkdownElement::new(markdown.clone(), MarkdownStyle::default()),
        );
        cx.run_until_parked();

        markdown.update(cx, |markdown, _| {
            assert_eq!(markdown.parsed_markdown.math_expressions.len(), 1);
            let rasterized = markdown.math_state.entries.values().any(|cached| {
                cached
                    .render_image
                    .get()
                    .is_some_and(|result| result.is_ok())
            });
            assert!(rasterized, "expected the equation to be rasterized");
        });
    }

    #[gpui::test]
    fn inline_math_is_not_shown_as_source(cx: &mut TestAppContext) {
        struct TestWindow;

        impl Render for TestWindow {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }

        ensure_theme_initialized(cx);
        let (_, cx) = cx.add_window_view(|_, _| TestWindow);
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                "before $x^2$ after".into(),
                None,
                None,
                MarkdownOptions {
                    render_math: true,
                    ..Default::default()
                },
                cx,
            )
        });
        cx.run_until_parked();
        let (rendered, _) = cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| MarkdownElement::new(markdown.clone(), MarkdownStyle::default()),
        );
        cx.run_until_parked();

        let text = rendered
            .text
            .lines
            .iter()
            .map(|line| line.layout.wrapped_text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains("$x^2$"),
            "formula delimiters leaked: {text:?}"
        );
        assert!(text.contains("before"), "missing leading text: {text:?}");
        assert!(text.contains("after"), "missing trailing text: {text:?}");
    }

    #[gpui::test]
    fn math_with_markdown_syntax_is_not_shown_as_source(cx: &mut TestAppContext) {
        struct TestWindow;

        impl Render for TestWindow {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }

        ensure_theme_initialized(cx);
        let (_, cx) = cx.add_window_view(|_, _| TestWindow);
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                "$$(f*g*h)(x)$$ tail\n\n$$\n\\frac{1}{2}\n$$\n".into(),
                None,
                None,
                MarkdownOptions {
                    render_math: true,
                    ..Default::default()
                },
                cx,
            )
        });
        cx.run_until_parked();
        let (rendered, _) = cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| MarkdownElement::new(markdown.clone(), MarkdownStyle::default()),
        );
        cx.run_until_parked();

        let text = rendered
            .text
            .lines
            .iter()
            .map(|line| line.layout.wrapped_text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains("$$") && !text.contains("(f*g"),
            "formula source leaked: {text:?}"
        );
        assert!(text.contains("tail"), "missing surrounding text: {text:?}");

        markdown.update(cx, |markdown, _| {
            assert_eq!(markdown.parsed_markdown.math_expressions.len(), 2);
            assert_eq!(
                markdown.math_state.entries.len(),
                2,
                "expected both formulas to have a cached render"
            );
        });
    }

    #[gpui::test]
    fn math_cache_evicts_previous_style_variants(cx: &mut TestAppContext) {
        struct TestWindow;

        impl Render for TestWindow {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }

        ensure_theme_initialized(cx);
        let (_, cx) = cx.add_window_view(|_, _| TestWindow);
        let markdown = cx.new(|cx| {
            Markdown::new_with_options(
                "$x^2$".into(),
                None,
                None,
                MarkdownOptions {
                    render_math: true,
                    ..Default::default()
                },
                cx,
            )
        });
        cx.run_until_parked();

        let mut first_style = MarkdownStyle::default();
        first_style.base_text_style.font_size = rems(1.0).into();
        cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| MarkdownElement::new(markdown.clone(), first_style),
        );

        let mut second_style = MarkdownStyle::default();
        second_style.base_text_style.font_size = rems(2.0).into();
        cx.draw(
            Default::default(),
            size(px(600.0), px(600.0)),
            |_window, _cx| MarkdownElement::new(markdown.clone(), second_style),
        );

        markdown.update(cx, |markdown, _| {
            assert_eq!(
                markdown.math_state.entries.len(),
                1,
                "only the current font and color variant should remain cached"
            );
        });
    }

    fn extract(markdown: &str) -> Vec<ParsedMarkdownMath> {
        let events = parse_markdown_with_options(markdown, false, false, false).events;
        extract_math_expressions(markdown, &events)
    }

    fn formulas(markdown: &str) -> Vec<(String, bool)> {
        extract(markdown)
            .into_iter()
            .map(|math| (math.contents.formula.to_string(), math.contents.display))
            .collect()
    }

    #[test]
    fn extracts_inline_math() {
        assert_eq!(
            formulas("hello $x^2$ world"),
            vec![("x^2".to_string(), false)]
        );
    }

    #[test]
    fn extracts_display_math() {
        assert_eq!(
            formulas("before\n\n$$\n\\frac{1}{2}\n$$\n\nafter"),
            vec![("\\frac{1}{2}".to_string(), true)]
        );
    }

    #[test]
    fn extracts_same_line_display_math() {
        assert_eq!(formulas("$$x^2$$"), vec![("x^2".to_string(), true)]);
    }

    #[test]
    fn extracts_fenced_math() {
        let parsed = extract("```math\n\\frac{1}{2}\n```\n\n```rust\nfn main() {}\n```");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].contents.formula, "\\frac{1}{2}");
        assert!(parsed[0].fenced);
    }

    #[test]
    fn extracts_multiple_inline_math() {
        assert_eq!(
            formulas("$a$ and $b$"),
            vec![("a".to_string(), false), ("b".to_string(), false)]
        );
    }

    #[test]
    fn extracts_backslash_delimited_math() {
        assert_eq!(
            formulas(r"before \(x^2\) after \[\frac{1}{2}\]"),
            vec![
                ("x^2".to_string(), false),
                ("\\frac{1}{2}".to_string(), true),
            ]
        );
    }

    #[test]
    fn ignores_escaped_backslash_math_delimiters() {
        assert!(formulas(r"before \\(not math\\) after").is_empty());
    }

    #[test]
    fn limits_the_number_of_rendered_formulas() {
        let markdown = "$x$ ".repeat(MAX_MATH_EXPRESSIONS + 10);
        assert_eq!(extract(&markdown).len(), MAX_MATH_EXPRESSIONS);
    }

    #[test]
    fn formula_limit_preserves_source_order_across_fenced_and_inline_math() {
        let markdown = format!(
            "$first$\n{}",
            "```math\nx\n```\n".repeat(MAX_MATH_EXPRESSIONS)
        );
        let expressions = extract(&markdown);

        assert_eq!(expressions.len(), MAX_MATH_EXPRESSIONS);
        assert_eq!(expressions[0].contents.formula.as_ref(), "first");
        assert!(!expressions[0].fenced);
    }

    #[test]
    fn does_not_break_dollar_links() {
        // Disabling pulldown-cmark's math extension keeps links containing `$`
        // parseable; our own scanner must not fabricate a formula here either.
        assert!(formulas("https://svelte.dev/docs/svelte/$state").is_empty());
    }

    #[test]
    fn ignores_currency() {
        assert!(formulas("It costs $5 and $10 today").is_empty());
        assert!(formulas("Prices: $100 and $200").is_empty());
    }

    #[test]
    fn ignores_escaped_dollars() {
        assert!(formulas(r"a \$x\$ b").is_empty());
    }

    #[test]
    fn ignores_code_spans_and_blocks() {
        assert!(formulas("a `$x$` b").is_empty());
        assert!(formulas("```\n$x$\n```").is_empty());
    }

    #[test]
    fn resolves_source_and_content_ranges() {
        let markdown = "a $x+y$ b";
        let parsed = extract(markdown);
        assert_eq!(parsed.len(), 1);
        assert_eq!(&markdown[parsed[0].source_range.clone()], "$x+y$");
        assert_eq!(&markdown[parsed[0].content_range.clone()], "x+y");
    }

    #[test]
    fn detects_block_math_paragraph() {
        let markdown = "$$\nx^2\n$$";
        let parsed = extract(markdown);
        let events = parse_markdown_with_options(markdown, false, false, false).events;
        let paragraph = events.first().map(|(range, _)| range.clone()).unwrap();
        assert!(block_math_for_paragraph(markdown, &parsed, &paragraph).is_some());
    }

    #[test]
    fn does_not_treat_inline_display_math_as_block() {
        let markdown = "text $$x^2$$ more";
        let parsed = extract(markdown);
        let events = parse_markdown_with_options(markdown, false, false, false).events;
        let paragraph = events.first().map(|(range, _)| range.clone()).unwrap();
        assert!(block_math_for_paragraph(markdown, &parsed, &paragraph).is_none());
    }

    fn shielded(markdown: &str) -> (Vec<ParsedMarkdownMath>, Vec<(Range<usize>, MarkdownEvent)>) {
        let mut events = parse_markdown_with_options(markdown, false, false, false).events;
        let expressions = extract_math_expressions(markdown, &events);
        shield_math_events(&mut events, &expressions);
        (expressions, events)
    }

    fn has_math_text_event(
        events: &[(Range<usize>, MarkdownEvent)],
        math: &ParsedMarkdownMath,
    ) -> bool {
        events.iter().any(|(range, event)| {
            matches!(event, MarkdownEvent::Text) && range == &math.source_range
        })
    }

    #[test]
    fn shields_markdown_syntax_inside_math() {
        // `*` and `[...]` would otherwise turn into emphasis and a link.
        let markdown = "$$(f*g*h)(x)$$ and $$[a+b](c+d)$$";
        let (expressions, events) = shielded(markdown);
        assert_eq!(expressions.len(), 2);
        for math in &expressions {
            assert!(
                has_math_text_event(&events, math),
                "formula was not shielded: {:?}",
                math.source_range
            );
        }
        assert!(
            !events.iter().any(|(_, event)| matches!(
                event,
                MarkdownEvent::Start(MarkdownTag::Emphasis | MarkdownTag::Link { .. })
            )),
            "markdown syntax leaked out of the formulas: {events:?}"
        );
    }

    #[test]
    fn shields_multiline_display_math_in_a_paragraph() {
        let markdown = "before\n$$\na & b \\\\\nc & d\n$$\nafter";
        let (expressions, events) = shielded(markdown);
        assert_eq!(expressions.len(), 1);
        assert!(has_math_text_event(&events, &expressions[0]));

        // The paragraph that wraps the formula must stay balanced.
        let starts = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::Start(MarkdownTag::Paragraph)))
            .count();
        let ends = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::End(MarkdownTagEnd::Paragraph)))
            .count();
        assert_eq!(starts, ends);
    }

    #[test]
    fn keeps_wrapping_tags_around_math() {
        let markdown = "_math $$x^2$$ in emphasis_ and **$$y$$ in bold**";
        let (expressions, events) = shielded(markdown);
        assert_eq!(expressions.len(), 2);
        for math in &expressions {
            assert!(has_math_text_event(&events, math));
        }
        assert_eq!(
            events
                .iter()
                .filter(|(_, event)| matches!(event, MarkdownEvent::Start(MarkdownTag::Emphasis)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|(_, event)| matches!(event, MarkdownEvent::Start(MarkdownTag::Strong)))
                .count(),
            1
        );
    }

    #[test]
    fn keeps_event_stream_balanced_when_shielding() {
        let markdown = "\
# Heading $$x^2$$ and **bold $$y$$**\n\
\n\
- item with $$z_1, z_2$$\n\
- [link $$w$$ text](https://example.com)\n\
\n\
> quote $$q$$\n\
\n\
| a | b |\n\
| - | - |\n\
| $$p$$ | $1 |\n\
\n\
![$$a^2+b^2=c^2$$](https://example.com/x.png)\n";
        let (expressions, events) = shielded(markdown);
        assert!(expressions.len() >= 5);
        for math in &expressions {
            assert!(
                has_math_text_event(&events, math),
                "formula was not shielded: {:?}",
                math.source_range
            );
            assert!(
                !events.iter().any(|(range, event)| {
                    matches!(
                        event,
                        MarkdownEvent::Start(_)
                            | MarkdownEvent::End(_)
                            | MarkdownEvent::RootStart
                            | MarkdownEvent::RootEnd(_)
                    ) && range.start >= math.source_range.start
                        && range.end <= math.source_range.end
                }),
                "a tag remained inside the formula at {:?}",
                math.source_range
            );
        }

        let starts = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::Start(_)))
            .count();
        let ends = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::End(_)))
            .count();
        assert_eq!(starts, ends, "unbalanced tags: {events:?}");

        let roots = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::RootStart))
            .count();
        let root_ends = events
            .iter()
            .filter(|(_, event)| matches!(event, MarkdownEvent::RootEnd(_)))
            .count();
        assert_eq!(roots, root_ends, "unbalanced root blocks: {events:?}");

        // An image whose alt text contains math must keep its wrapping tag.
        assert_eq!(
            events
                .iter()
                .filter(|(_, event)| matches!(
                    event,
                    MarkdownEvent::Start(MarkdownTag::Image { .. })
                ))
                .count(),
            1,
            "image tag was dropped while shielding: {events:?}"
        );
    }

    #[test]
    fn applies_page_level_macros_to_other_formulas() {
        let markdown = "$$\n\\newcommand{\\sca}[1]{\\langle #1 \\rangle}\n|\\sca{x,y}|^2\n$$\n\nand $$\\sca{\\cdot,\\cdot}$$";
        let parsed = extract(markdown);
        assert_eq!(parsed.len(), 2);
        assert!(
            parsed[0].contents.formula.contains("|\\sca{x,y}|^2"),
            "defining formula lost its body: {}",
            parsed[0].contents.formula
        );
        assert!(
            parsed[1].contents.formula.contains("\\newcommand{\\sca}"),
            "macro definition was not carried over: {}",
            parsed[1].contents.formula
        );

        // A formula that only defines a macro draws nothing.
        let only_definition = extract("$$\\newcommand{\\unused}{x}$$");
        assert!(
            only_definition[0].contents.formula.trim().is_empty(),
            "definition-only formula should be empty: {}",
            only_definition[0].contents.formula
        );
    }

    #[test]
    fn strips_equation_labels_and_renders_references_as_text() {
        assert_eq!(
            formulas("$$\\int_0^x \\sin(x) dx \\label{eq:test}$$"),
            vec![("\\int_0^x \\sin(x) dx".to_string(), true)]
        );
        assert_eq!(
            formulas("$$\\eqref{eq:test}$$"),
            vec![("\\text{(eq:test)}".to_string(), true)]
        );
    }

    #[test]
    fn renders_multiline_and_markdown_heavy_math_to_svg() {
        for formula in [
            "\\begin{array}{cc}a&b\\\\c&d\\end{array}",
            "\\text{while $e^2$ do $c^2$ end}",
            "\\left|\\begin{array}{cc}a&b\\\\c&d\\end{array}\\right|",
        ] {
            let svg = render_formula_to_svg(
                formula,
                true,
                gpui::px(16.0),
                gpui::hsla(0.0, 0.0, 0.0, 1.0),
            )
            .unwrap_or_else(|error| panic!("failed to render {formula:?}: {error}"));
            assert!(svg.contains("<svg"), "no svg produced for {formula:?}");
        }
    }
}
