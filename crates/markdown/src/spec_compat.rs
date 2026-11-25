//! Opt-in conformance harness against the published Markdown example suites.
//!
//! Three layers are measured, and each one answers a different question:
//!
//! 1. **Engine** — `pulldown-cmark` driven with this crate's [`PARSE_OPTIONS`],
//!    rendered by `pulldown_cmark::html`. Answers "how conformant is the parser
//!    configuration we ship", with no wrapper in the way.
//! 2. **Crate** — [`parse_markdown_with_options`] output, re-serialized to HTML
//!    by the emitter below. Answers "does our event mapping preserve what the
//!    engine parsed". Differences against layer 1 and against the suites are
//!    the deliberate adaptations (autolinked URLs, path-looking fenced code
//!    blocks, HTML block handling).
//! 3. **Pathological inputs** — deep nesting and bracket runs from
//!    `cmark-gfm/test/pathological_tests.py`, asserting bounded time instead of
//!    matching output.
//!
//! The suites are not vendored: they are third-party data under their own
//! licenses, and offline CI must not depend on a download. `script/markdown-spec`
//! fetches them into `target/markdown-spec/` (or `$ZZZ_MARKDOWN_SPEC_DIR`) and
//! runs this file; without fixtures the suite tests print a skip notice and pass.
//!
//! The `*_MIN_PASSED` constants are a ratchet: they record the measured
//! baseline, so a regression fails the test while an improvement only changes
//! the printed report until someone raises the constant.
//!
//! Remaining mismatches, all attributed:
//!
//! - **Smart punctuation (25 CommonMark examples).** `ENABLE_SMART_PUNCTUATION`
//!   turns quotes, dashes and ellipses into typographic characters. The same
//!   suite passes 652/652 with that option off, which the test asserts.
//! - **Bare URLs are autolinked (3 CommonMark, 3 GFM examples).** `parser` runs
//!   `linkify` over every text run, so `https://example.com` in prose becomes a
//!   link where CommonMark leaves it as text. This is the GFM behaviour, but it
//!   does not implement GFM's full rule set: `www.` prefixes stay plain text
//!   (10 GFM examples) and neither does tagfilter (1 example).
//! - **`parse_html` drops inline HTML (15 examples per suite).** Raw inline
//!   HTML never reaches the renderer in that mode; it is dropped rather than
//!   passed through, and block-level HTML goes to the restricted
//!   `html::html_parser`. `raw_html_keeps_the_restricted_path` pins that
//!   policy, because passing these examples could mean containment regressed.
//! - **GFM 0.29 predates CommonMark 0.31.2.** Nested `****` emphasis and the
//!   empty `<tbody>` the reference renderer emits are revision differences, not
//!   parser bugs.
//!
//! Two behaviours are deliberate and are reproduced by this file rather than
//! reported as mismatches: fenced code keeps its whole info string (mermaid and
//! math fences read `startline=`/scale metadata from it) while the language is
//! resolved from that string's first word, and `` ` foo ` `` loses its padding
//! space through `parser::strip_code_span_padding`.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use pulldown_cmark::{Alignment, Options, Parser, html};

use crate::parser::{
    MarkdownEvent, MarkdownTag, MarkdownTagEnd, PARSE_OPTIONS, parse_markdown_with_options,
};

/// Measured against CommonMark 0.31.2 with [`PARSE_OPTIONS`]. All 25 mismatches
/// are `ENABLE_SMART_PUNCTUATION` rewriting quotes, dashes and ellipses, which
/// is why the same suite run with that option off (asserted in
/// `commonmark_spec_examples`) matches every example.
const COMMONMARK_ENGINE_MIN_PASSED: usize = 627;
/// Measured against CommonMark 0.31.2 through [`parse_markdown_with_options`].
/// On top of the 25 smart-punctuation mismatches: 3 bare-URL autolinks.
const COMMONMARK_CRATE_MIN_PASSED: usize = 624;
/// The same, with `parse_html` on: raw inline HTML is stripped, so most of the
/// Raw HTML and HTML block examples differ by design.
const COMMONMARK_CRATE_MIN_PASSED_PARSE_HTML: usize = 609;

/// Measured against the GFM 0.29 suite, which predates CommonMark 0.31.2: part
/// of the gap is the revision difference (nested `****` emphasis, empty table
/// bodies), part is missing extensions (GFM autolinks, tagfilter).
const GFM_ENGINE_MIN_PASSED: usize = 623;
const GFM_CRATE_MIN_PASSED: usize = 623;
const GFM_CRATE_MIN_PASSED_PARSE_HTML: usize = 608;

/// Fetched by `script/markdown-spec`.
const COMMONMARK_SPEC_FILE: &str = "commonmark-spec-0.31.2.json";
const GFM_SPEC_FILE: &str = "gfm-spec.txt";

struct SpecExample {
    id: String,
    section: String,
    markdown: String,
    html: String,
}

fn fixtures_dir() -> PathBuf {
    match std::env::var_os("ZZZ_MARKDOWN_SPEC_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target/markdown-spec"),
    }
}

fn load_fixtures(file: &str, loader: fn(&str) -> Vec<SpecExample>) -> Option<Vec<SpecExample>> {
    let path = fixtures_dir().join(file);
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!(
            "skipping: {} is missing; run script/markdown-spec to fetch the suites",
            path.display()
        );
        return None;
    };
    Some(loader(&text))
}

fn load_commonmark(text: &str) -> Vec<SpecExample> {
    let examples: serde_json::Value =
        serde_json::from_str(text).expect("commonmark spec json should parse");
    examples
        .as_array()
        .expect("commonmark spec should be an array")
        .iter()
        .map(|example| SpecExample {
            id: format!("commonmark example {}", example["example"]),
            section: example["section"].as_str().expect("section").to_owned(),
            markdown: example["markdown"].as_str().expect("markdown").to_owned(),
            html: example["html"].as_str().expect("html").to_owned(),
        })
        .collect()
}

fn load_gfm(text: &str) -> Vec<SpecExample> {
    const FENCE: &str = "````````````````````````````````";
    let mut examples = Vec::new();
    let mut section = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_owned();
            continue;
        }
        let Some(kind) = line
            .strip_prefix(FENCE)
            .map(str::trim)
            .and_then(|rest| rest.strip_prefix("example"))
        else {
            continue;
        };
        let kind = kind.trim();

        let mut markdown = String::new();
        let mut expected_html = String::new();
        let mut in_html = false;
        for line in lines.by_ref() {
            if line == FENCE {
                break;
            }
            if !in_html && line == "." {
                in_html = true;
                continue;
            }
            let target = if in_html {
                &mut expected_html
            } else {
                &mut markdown
            };
            target.push_str(line);
            target.push('\n');
        }

        // The suite writes a literal tab as an arrow so it survives editors.
        let example = examples.len() + 1;
        examples.push(SpecExample {
            id: format!("gfm example {example} ({kind})"),
            section: section.clone(),
            markdown: markdown.replace('\u{2192}', "\t"),
            html: expected_html.replace('\u{2192}', "\t"),
        });
    }
    examples
}

fn engine_html(markdown: &str, options: Options) -> String {
    let mut output = String::new();
    html::push_html(&mut output, Parser::new_ext(markdown, options));
    output
}

/// The text of each HTML block as the engine reports it, which is what the
/// restricted HTML parser receives. A block's source span is contiguous, so it
/// keeps the block quote markers of every line but the first; the engine's own
/// per-line events do not, and that is the difference this function removes.
fn html_block_sources(markdown: &str) -> BTreeMap<usize, String> {
    let mut sources = BTreeMap::new();
    let mut parser = Parser::new_ext(markdown, PARSE_OPTIONS).into_offset_iter();
    while let Some((event, range)) = parser.next() {
        if !matches!(
            event,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::HtmlBlock)
        ) {
            continue;
        }

        let mut html = String::new();
        for (event, _) in parser.by_ref() {
            match event {
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::HtmlBlock) => break,
                pulldown_cmark::Event::Html(chunk) => html.push_str(&chunk),
                _ => {}
            }
        }
        sources.insert(range.start, html);
    }
    sources
}

fn crate_html(markdown: &str, parse_html: bool) -> String {
    let parsed = parse_markdown_with_options(markdown, parse_html, false, false);
    let mut emitter = Emitter {
        html_block_sources: html_block_sources(markdown),
        ..Emitter::default()
    };
    let mut output = String::new();
    for (range, event) in &parsed.events {
        emitter.event(markdown, range.clone(), event, &mut output);
    }
    output
}

fn escape_html(text: &str, output: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            ch => output.push(ch),
        }
    }
}

/// Percent-encodes a URL destination the way the specification's reference
/// implementation does, so that `[link](/föö)` is compared as
/// `href="/f%C3%B6%C3%B6"` rather than as a mismatch.
fn escape_href(href: &str, output: &mut String) {
    for byte in href.bytes() {
        match byte {
            b'&' => output.push_str("&amp;"),
            b'\'' => output.push_str("&#x27;"),
            b'!' | b'#' | b'$' | b'%' | b'(' | b')' | b'*' | b'+' | b',' | b'-' | b'.' | b'/'
            | b':' | b';' | b'=' | b'?' | b'@' | b'_' | b'~' => output.push(byte as char),
            byte if byte.is_ascii_alphanumeric() => output.push(byte as char),
            byte => {
                let _ = write!(output, "%{byte:02X}");
            }
        }
    }
}

struct OpenImage {
    dest_url: String,
    title: String,
    alt: String,
}

/// Serializes this crate's event stream the way the specification's reference
/// implementation would, so that the comparison measures the parse rather than
/// the GPUI layout.
#[derive(Default)]
struct Emitter {
    images: Vec<OpenImage>,
    table_alignments: Vec<Alignment>,
    table_cells: usize,
    in_table_head: bool,
    html_block: Option<(usize, bool)>,
    html_block_sources: BTreeMap<usize, String>,
    footnote_number: usize,
}

impl Emitter {
    fn event(
        &mut self,
        source: &str,
        range: std::ops::Range<usize>,
        event: &MarkdownEvent,
        out: &mut String,
    ) {
        let text = || source.get(range.clone()).unwrap_or_default();

        // The specification flattens everything inside an image to its alt text,
        // so nested markup and links are dropped rather than serialized.
        match event {
            MarkdownEvent::Start(MarkdownTag::Image {
                dest_url, title, ..
            }) => {
                self.images.push(OpenImage {
                    dest_url: dest_url.to_string(),
                    title: title.to_string(),
                    alt: String::new(),
                });
                return;
            }
            MarkdownEvent::End(MarkdownTagEnd::Image) => {
                self.close_image(out);
                return;
            }
            _ => {}
        }
        if !self.images.is_empty() {
            match event {
                MarkdownEvent::Text | MarkdownEvent::Code => self.push_text(text(), out),
                MarkdownEvent::SubstitutedText(substituted)
                | MarkdownEvent::SubstitutedCode(substituted) => self.push_text(substituted, out),
                MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => self.push_text(" ", out),
                _ => {}
            }
            return;
        }

        match event {
            MarkdownEvent::RootStart | MarkdownEvent::RootEnd(_) => {}
            MarkdownEvent::Text => self.push_text(text(), out),
            MarkdownEvent::SubstitutedText(substituted) => self.push_text(substituted, out),
            MarkdownEvent::Code => {
                out.push_str("<code>");
                escape_html(text(), out);
                out.push_str("</code>");
            }
            MarkdownEvent::SubstitutedCode(substituted) => {
                out.push_str("<code>");
                escape_html(substituted, out);
                out.push_str("</code>");
            }
            MarkdownEvent::Html => {
                if let Some((_, emitted_inner)) = self.html_block.as_mut() {
                    *emitted_inner = true;
                }
                out.push_str(text());
            }
            MarkdownEvent::InlineHtml => out.push_str(text()),
            MarkdownEvent::SoftBreak => out.push('\n'),
            MarkdownEvent::HardBreak => out.push_str("<br />\n"),
            MarkdownEvent::Rule => out.push_str("<hr />\n"),
            MarkdownEvent::TaskListMarker(checked) => {
                out.push_str("<input disabled=\"\" type=\"checkbox\"");
                if *checked {
                    out.push_str(" checked=\"\"");
                }
                out.push_str("/> ");
            }
            MarkdownEvent::FootnoteReference(label) => {
                self.footnote_number += 1;
                let _ = write!(
                    out,
                    "<sup class=\"footnote-reference\"><a href=\"#{label}\">{}</a></sup>",
                    self.footnote_number
                );
            }
            MarkdownEvent::Start(tag) => self.start_tag(range, tag, out),
            MarkdownEvent::End(tag) => self.end_tag(tag, out),
        }
    }

    fn push_text(&mut self, text: &str, out: &mut String) {
        match self.images.last_mut() {
            Some(image) => image.alt.push_str(text),
            None => escape_html(text, out),
        }
    }

    fn close_image(&mut self, out: &mut String) {
        let Some(image) = self.images.pop() else {
            return;
        };
        if let Some(parent) = self.images.last_mut() {
            parent.alt.push_str(&image.alt);
            return;
        }

        out.push_str("<img src=\"");
        escape_href(&image.dest_url, out);
        out.push_str("\" alt=\"");
        escape_html(&image.alt, out);
        out.push('"');
        if !image.title.is_empty() {
            out.push_str(" title=\"");
            escape_html(&image.title, out);
            out.push('"');
        }
        out.push_str(" />");
    }

    fn start_tag(&mut self, range: std::ops::Range<usize>, tag: &MarkdownTag, out: &mut String) {
        match tag {
            MarkdownTag::Paragraph => out.push_str("<p>"),
            MarkdownTag::Heading {
                level,
                id,
                classes,
                attrs,
            } => {
                let _ = write!(out, "<h{}", heading_number(*level));
                if let Some(id) = id {
                    out.push_str(" id=\"");
                    escape_html(id, out);
                    out.push('"');
                }
                if !classes.is_empty() {
                    out.push_str(" class=\"");
                    let mut first = true;
                    for class in classes {
                        if !first {
                            out.push(' ');
                        }
                        first = false;
                        escape_html(class, out);
                    }
                    out.push('"');
                }
                for (key, value) in attrs {
                    out.push(' ');
                    escape_html(key, out);
                    if let Some(value) = value {
                        out.push_str("=\"");
                        escape_html(value, out);
                        out.push('"');
                    }
                }
                out.push('>');
            }
            MarkdownTag::BlockQuote(_) => out.push_str("<blockquote>\n"),
            MarkdownTag::CodeBlock { kind, .. } => {
                out.push_str("<pre><code");
                if let crate::parser::CodeBlockKind::FencedLang(info) = kind {
                    // The renderer resolves a fence's language from the first word of its
                    // info string, and keeps the rest for fence metadata such as
                    // `startline=3` (see `languages_by_name` in markdown.rs), so the class
                    // has to name that word rather than the whole info string.
                    let language = info.split_whitespace().next().unwrap_or_default();
                    out.push_str(" class=\"language-");
                    escape_html(language, out);
                    out.push('"');
                }
                out.push('>');
            }
            MarkdownTag::HtmlBlock => self.html_block = Some((range.start, false)),
            MarkdownTag::List(first) => match first {
                Some(1) => out.push_str("<ol>\n"),
                Some(first) => {
                    let _ = writeln!(out, "<ol start=\"{first}\">");
                }
                None => out.push_str("<ul>\n"),
            },
            MarkdownTag::Item => out.push_str("<li>"),
            MarkdownTag::FootnoteDefinition(label) => {
                let _ = write!(out, "<div class=\"footnote-definition\" id=\"");
                escape_html(label, out);
                out.push_str("\">");
            }
            MarkdownTag::Table(alignments) => {
                self.table_alignments = alignments.clone();
                self.table_cells = 0;
                out.push_str("<table>\n");
            }
            MarkdownTag::TableHead => {
                self.in_table_head = true;
                self.table_cells = 0;
                out.push_str("<thead>\n<tr>\n");
            }
            MarkdownTag::TableRow => {
                self.table_cells = 0;
                out.push_str("<tr>\n");
            }
            MarkdownTag::TableCell => {
                let alignment = self.table_alignments.get(self.table_cells).copied();
                self.table_cells += 1;
                out.push_str(if self.in_table_head { "<th" } else { "<td" });
                match alignment {
                    Some(Alignment::Left) => out.push_str(" align=\"left\""),
                    Some(Alignment::Center) => out.push_str(" align=\"center\""),
                    Some(Alignment::Right) => out.push_str(" align=\"right\""),
                    _ => {}
                }
                out.push('>');
            }
            MarkdownTag::Emphasis => out.push_str("<em>"),
            MarkdownTag::Strong => out.push_str("<strong>"),
            MarkdownTag::Strikethrough => out.push_str("<del>"),
            MarkdownTag::Superscript => out.push_str("<sup>"),
            MarkdownTag::Subscript => out.push_str("<sub>"),
            MarkdownTag::Link {
                dest_url, title, ..
            } => {
                out.push_str("<a href=\"");
                escape_href(dest_url, out);
                out.push('"');
                if !title.is_empty() {
                    out.push_str(" title=\"");
                    escape_html(title, out);
                    out.push('"');
                }
                out.push('>');
            }
            // Images are opened and closed in `event`, before tag dispatch, so
            // that the same code handles an image nested in another's alt text.
            MarkdownTag::Image { .. } => {}
            MarkdownTag::MetadataBlock(_) => {}
            MarkdownTag::DefinitionList => out.push_str("<dl>\n"),
            MarkdownTag::DefinitionListTitle => out.push_str("<dt>"),
            MarkdownTag::DefinitionListDefinition => out.push_str("<dd>"),
        }
    }

    fn end_tag(&mut self, tag: &MarkdownTagEnd, out: &mut String) {
        match tag {
            MarkdownTagEnd::Paragraph => out.push_str("</p>\n"),
            MarkdownTagEnd::Heading(level) => {
                let _ = writeln!(out, "</h{}>", heading_number(*level));
            }
            MarkdownTagEnd::BlockQuote(_) => out.push_str("</blockquote>\n"),
            MarkdownTagEnd::CodeBlock => out.push_str("</code></pre>\n"),
            // `parse_html` consumes the events inside an HTML block, leaving only
            // the block's span, so the raw source stands in for the GPUI renderer
            // that normally handles these blocks.
            MarkdownTagEnd::HtmlBlock => {
                if let Some((start, emitted_inner)) = self.html_block.take()
                    && !emitted_inner
                    && let Some(html) = self.html_block_sources.get(&start)
                {
                    out.push_str(html);
                }
            }
            MarkdownTagEnd::List(ordered) => {
                out.push_str(if *ordered { "</ol>\n" } else { "</ul>\n" })
            }
            MarkdownTagEnd::Item => out.push_str("</li>\n"),
            MarkdownTagEnd::FootnoteDefinition => out.push_str("</div>\n"),
            MarkdownTagEnd::Table => out.push_str("</table>\n"),
            MarkdownTagEnd::TableHead => {
                self.in_table_head = false;
                out.push_str("</tr>\n</thead>\n");
            }
            MarkdownTagEnd::TableRow => out.push_str("</tr>\n"),
            MarkdownTagEnd::TableCell => out.push_str(if self.in_table_head {
                "</th>\n"
            } else {
                "</td>\n"
            }),
            MarkdownTagEnd::Emphasis => out.push_str("</em>"),
            MarkdownTagEnd::Strong => out.push_str("</strong>"),
            MarkdownTagEnd::Strikethrough => out.push_str("</del>"),
            MarkdownTagEnd::Superscript => out.push_str("</sup>"),
            MarkdownTagEnd::Subscript => out.push_str("</sub>"),
            MarkdownTagEnd::Link => out.push_str("</a>"),
            MarkdownTagEnd::Image => {}
            MarkdownTagEnd::MetadataBlock(_) => {}
            MarkdownTagEnd::DefinitionList => out.push_str("</dl>\n"),
            MarkdownTagEnd::DefinitionListTitle => out.push_str("</dt>\n"),
            MarkdownTagEnd::DefinitionListDefinition => out.push_str("</dd>\n"),
        }
    }
}

fn heading_number(level: pulldown_cmark::HeadingLevel) -> u8 {
    match level {
        pulldown_cmark::HeadingLevel::H1 => 1,
        pulldown_cmark::HeadingLevel::H2 => 2,
        pulldown_cmark::HeadingLevel::H3 => 3,
        pulldown_cmark::HeadingLevel::H4 => 4,
        pulldown_cmark::HeadingLevel::H5 => 5,
        pulldown_cmark::HeadingLevel::H6 => 6,
    }
}

/// Elements whose whitespace-only text children carry no meaning.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "html"
            | "body"
            | "div"
            | "p"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "td"
            | "th"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "pre"
            | "dl"
            | "dt"
            | "dd"
    )
}

/// Parses both sides into a DOM and prints a canonical form, so that attribute
/// order, self-closing slashes, entity spelling and inter-tag whitespace do not
/// count as conformance failures while structure and text still do.
fn canonical_html(html: &str) -> String {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("inline html should parse");
    let mut canonical = String::new();
    write_canonical(&dom.document, false, &mut canonical);
    canonical
}

fn write_canonical(node: &Handle, in_pre: bool, out: &mut String) {
    match &node.data {
        NodeData::Document => write_children(&node.children.borrow(), in_pre, true, out),
        NodeData::Doctype { .. } | NodeData::ProcessingInstruction { .. } => {}
        NodeData::Comment { contents } => {
            let _ = write!(out, "<!--{}-->", contents.trim());
        }
        NodeData::Text { contents } => {
            write_text(&contents.borrow(), in_pre, true, true, out);
        }
        NodeData::Element { name, attrs, .. } => {
            let name = name.local.to_string();
            let mut attrs = attrs
                .borrow()
                .iter()
                .filter(|attr| !attr.name.local.to_string().starts_with("data-"))
                .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                .collect::<Vec<_>>();
            attrs.sort();
            let attrs = attrs
                .into_iter()
                .map(|(name, value)| format!(" {name}=\"{}\"", value.trim()))
                .collect::<String>();
            let children = node.children.borrow();
            let empty = children.is_empty();
            if name == "tbody" && empty {
                return;
            }
            let _ = write!(out, "<{name}{attrs}>");
            write_children(&children, in_pre || name == "pre", is_block(&name), out);
            drop(children);
            let _ = write!(out, "</{name}>");
            if is_block(&name) && !empty {
                out.push('\n');
            }
        }
    }
}

/// `parent_is_block` gates the edge trimming: space at the edge of a block
/// container is invisible, but the same space inside an inline container such
/// as `<code>` is part of the text and a difference there is a real mismatch.
fn write_children(children: &[Handle], in_pre: bool, parent_is_block: bool, out: &mut String) {
    for (index, child) in children.iter().enumerate() {
        match &child.data {
            NodeData::Text { contents } => {
                write_text(
                    &contents.borrow(),
                    in_pre,
                    parent_is_block && block_boundary(children, index, false),
                    parent_is_block && block_boundary(children, index, true),
                    out,
                );
            }
            _ => write_canonical(child, in_pre, out),
        }
    }
}

fn write_text(contents: &str, in_pre: bool, trim_start: bool, trim_end: bool, out: &mut String) {
    if in_pre {
        if !contents.is_empty() {
            let _ = write!(out, "\"{}\"", contents.replace('"', "\\\""));
        }
        return;
    }

    let mut text = collapse_whitespace(contents);
    if trim_start {
        text = text.trim_start().to_owned();
    }
    if trim_end {
        text = text.trim_end().to_owned();
    }
    if text.is_empty() {
        return;
    }
    let _ = write!(out, "\"{}\"", text.replace('"', "\\\""));
}

/// Whether the whitespace between a text node and its nearest non-empty
/// neighbour is insignificant, which it is at the edge of a block container and
/// next to a block element, but not between two inline nodes: `foo *bar*` loses
/// its space there and that difference must still be reported.
fn block_boundary(children: &[Handle], index: usize, forward: bool) -> bool {
    let mut index = index as isize;
    loop {
        index += if forward { 1 } else { -1 };
        if index < 0 {
            return true;
        }
        let Some(sibling) = children.get(index as usize) else {
            return true;
        };
        match &sibling.data {
            // Whitespace-only text between the boundary and here is invisible.
            NodeData::Text { contents } if contents.borrow().trim().is_empty() => {}
            NodeData::Element { name, .. } => return is_block(&name.local),
            _ => return false,
        }
    }
}

fn collapse_whitespace(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut in_whitespace = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                collapsed.push(' ');
            }
            in_whitespace = true;
        } else {
            collapsed.push(ch);
            in_whitespace = false;
        }
    }
    collapsed
}

struct Report {
    name: &'static str,
    total: usize,
    failures: Vec<(String, String, String)>,
}

impl Report {
    fn run(name: &'static str, examples: &[SpecExample], render: impl Fn(&str) -> String) -> Self {
        let failures = examples
            .iter()
            .filter_map(|example| {
                let expected = canonical_html(&example.html);
                let actual = canonical_html(&render(&example.markdown));
                (expected != actual).then(|| {
                    (
                        example.id.clone(),
                        example.section.clone(),
                        first_difference(&expected, &actual),
                    )
                })
            })
            .collect();

        let report = Self {
            name,
            total: examples.len(),
            failures,
        };
        println!(
            "{}: {}/{} examples match",
            report.name,
            report.total - report.failures.len(),
            report.total
        );
        let mut by_section: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for example in examples {
            by_section.entry(example.section.as_str()).or_default().0 += 1;
        }
        for (_, section, _) in &report.failures {
            by_section.entry(section.as_str()).or_default().1 += 1;
        }
        for (section, (total, failed)) in &by_section {
            if *failed > 0 {
                println!("  {failed:3}/{total:3} mismatched  {section}");
            }
        }
        if !report.failures.is_empty() {
            println!("  mismatches:");
            for (id, _, difference) in report.failures.iter().take(report_limit()) {
                println!("    {id}: {difference}");
            }
        }
        report
    }

    fn passed(&self) -> usize {
        self.total - self.failures.len()
    }

    fn assert_ratchet(&self, minimum: usize) {
        assert!(
            self.passed() >= minimum,
            "{}: {} examples match but the recorded baseline is {minimum}; the printed report \
             lists the regressions. Raise the baseline only after deciding a mismatch is \
             acceptable.",
            self.name,
            self.passed()
        );
    }
}

/// `ZZZ_MARKDOWN_SPEC_DUMP` prints every mismatch instead of the first ten;
/// useful when deciding whether a mismatch is a renderer gap or a real one.
fn report_limit() -> usize {
    std::env::var("ZZZ_MARKDOWN_SPEC_DUMP")
        .ok()
        .and_then(|limit| limit.parse().ok())
        .unwrap_or(10)
}

fn first_difference(expected: &str, actual: &str) -> String {
    const CONTEXT: usize = 60;
    let position = expected
        .bytes()
        .zip(actual.bytes())
        .position(|(expected, actual)| expected != actual)
        .unwrap_or(expected.len().min(actual.len()));
    let expected = truncate_around(expected, position, CONTEXT);
    let actual = truncate_around(actual, position, CONTEXT);
    format!("expected …{expected}… but got …{actual}…")
}

fn truncate_around(text: &str, position: usize, context: usize) -> &str {
    let start = text
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index + context < position)
        .last()
        .unwrap_or(0);
    let end = text
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index > position + context)
        .unwrap_or(text.len());
    &text[start..end]
}

#[test]
fn commonmark_spec_examples() {
    let Some(examples) = load_fixtures(COMMONMARK_SPEC_FILE, load_commonmark) else {
        return;
    };
    assert_eq!(
        examples.len(),
        652,
        "unexpected CommonMark 0.31.2 example count"
    );

    let engine = Report::run(
        "commonmark engine (pulldown-cmark + PARSE_OPTIONS)",
        &examples,
        |markdown| engine_html(markdown, PARSE_OPTIONS),
    );
    engine.assert_ratchet(COMMONMARK_ENGINE_MIN_PASSED);

    // Pinning the cause of the 25 mismatches: with smart punctuation off, the
    // configured engine matches the whole suite.
    let plain_engine = Report::run(
        "commonmark engine (smart punctuation off)",
        &examples,
        |markdown| engine_html(markdown, PARSE_OPTIONS - Options::ENABLE_SMART_PUNCTUATION),
    );
    plain_engine.assert_ratchet(examples.len());

    let crate_layer = Report::run("commonmark crate (event stream)", &examples, |markdown| {
        crate_html(markdown, false)
    });
    crate_layer.assert_ratchet(COMMONMARK_CRATE_MIN_PASSED);

    let crate_layer_html = Report::run(
        "commonmark crate (event stream, parse_html)",
        &examples,
        |markdown| crate_html(markdown, true),
    );
    crate_layer_html.assert_ratchet(COMMONMARK_CRATE_MIN_PASSED_PARSE_HTML);
}

#[test]
fn gfm_spec_examples() {
    let Some(examples) = load_fixtures(GFM_SPEC_FILE, load_gfm) else {
        return;
    };
    assert_eq!(examples.len(), 672, "unexpected GFM 0.29 example count");

    let engine = Report::run(
        "gfm engine (pulldown-cmark + PARSE_OPTIONS)",
        &examples,
        |markdown| engine_html(markdown, PARSE_OPTIONS),
    );
    engine.assert_ratchet(GFM_ENGINE_MIN_PASSED);

    let crate_layer = Report::run("gfm crate (event stream)", &examples, |markdown| {
        crate_html(markdown, false)
    });
    crate_layer.assert_ratchet(GFM_CRATE_MIN_PASSED);

    let crate_layer_html = Report::run(
        "gfm crate (event stream, parse_html)",
        &examples,
        |markdown| crate_html(markdown, true),
    );
    crate_layer_html.assert_ratchet(GFM_CRATE_MIN_PASSED_PARSE_HTML);
}

/// Inputs from `cmark-gfm/test/pathological_tests.py`, which guard the parser
/// against quadratic backtracking. The assertion is a wall-clock bound rather
/// than an output comparison: the point is that none of these hang or panic,
/// because a chat message can contain any of them.
#[test]
fn pathological_inputs_finish_quickly() {
    let deep_quotes = "> ".repeat(50_000) + "a";
    let deep_lists = (0..1_000)
        .map(|depth| "  ".repeat(depth) + "* a\n")
        .collect::<String>();
    let deep_brackets = "[".repeat(50_000) + "a" + &"]".repeat(50_000);
    let deep_links = "[a](b".repeat(30_000);
    let deep_images = "![[]()".repeat(40_000);
    let emphasis_runs = (1..1_000)
        .map(|count| "e".to_owned() + &"`".repeat(count))
        .collect::<String>();
    let asterisk_runs = "*".repeat(100_000);
    let reference_collisions = (0..5_000)
        .map(|index| format!("[k{index}]: /url\n\n[k{}]\n\n", (index * 7919) % 5_000))
        .collect::<String>();
    let mixed_pattern = "[ (](".repeat(20_000);
    let nul_bytes = "abc\u{0}de\u{0}".repeat(1_000);

    let cases: [(&str, String); 10] = [
        ("deep block quotes", deep_quotes),
        ("deep lists", deep_lists),
        ("deep brackets", deep_brackets),
        ("unterminated links", deep_links),
        ("repeated images", deep_images),
        ("backtick runs", emphasis_runs),
        ("asterisk runs", asterisk_runs),
        ("reference collisions", reference_collisions),
        ("mixed pattern", mixed_pattern),
        ("nul bytes", nul_bytes),
    ];

    for (name, markdown) in cases {
        let start = std::time::Instant::now();
        let parsed = parse_markdown_with_options(&markdown, true, true, true);
        let elapsed = start.elapsed();
        println!("{name}: {} events in {elapsed:?}", parsed.events.len());
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "{name} took {elapsed:?}"
        );
    }
}

/// Raw HTML is the one part of the specification this crate does not reproduce
/// verbatim: with `parse_html`, block-level HTML goes through the restricted
/// `html::html_parser` and inline HTML is dropped, so nothing from a document
/// reaches the renderer as interpreted markup. The suites above show the cost
/// of that policy; this pins the policy itself, because a change that starts
/// passing the Raw HTML examples could be a regression in containment rather
/// than conformance progress.
#[test]
fn raw_html_keeps_the_restricted_path() {
    let hostile = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "<iframe src=\"javascript:alert(1)\"></iframe>",
        "<div onclick=\"alert(1)\">click</div>",
        "<!-- <script>alert(1)</script> -->",
        "> <div>\n> payload\n",
    ];

    for markdown in hostile {
        let parsed = parse_markdown_with_options(markdown, true, false, false);
        for (range, event) in &parsed.events {
            assert!(
                !matches!(event, MarkdownEvent::Html),
                "{markdown:?} passed raw HTML through at {range:?}"
            );
        }

        let parsed_raw = parse_markdown_with_options(markdown, false, false, false);
        let has_html_block = parsed_raw
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::Start(MarkdownTag::HtmlBlock)));
        if has_html_block {
            assert!(
                !parsed.html_blocks.is_empty(),
                "{markdown:?} produced an HTML block without a restricted parse"
            );
        }
    }
}
