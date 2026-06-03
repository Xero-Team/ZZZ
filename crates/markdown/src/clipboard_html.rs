use std::ops::Range;

use gpui::{FontWeight, SharedString, TextAlign};
use pulldown_cmark::{Alignment, HeadingLevel};

use crate::ParsedMarkdown;
use crate::html::html_parser::{
    HtmlHighlightStyle, HtmlImage, HtmlParagraph, HtmlParagraphChunk, ParsedHtmlBlock,
    ParsedHtmlElement, ParsedHtmlTable, ParsedHtmlTableColumn, ParsedHtmlTableRow, ParsedHtmlText,
};
use crate::parser::{MarkdownEvent, MarkdownTag};

pub(crate) fn html_fragment_for_selection(
    parsed: &ParsedMarkdown,
    selection: Range<usize>,
    plain_text: &str,
    resolve_image_src: Option<&dyn Fn(&str) -> Option<String>>,
) -> String {
    match try_html_fragment_for_selection(parsed, selection, resolve_image_src) {
        Ok(html) => html,
        Err(error) => {
            log::debug!("Falling back to plain-text clipboard HTML: {error:?}");
            basic_html_from_plain_text(plain_text)
        }
    }
}

pub(crate) fn plain_text_for_selection(parsed: &ParsedMarkdown, selection: Range<usize>) -> String {
    match try_plain_text_for_selection(parsed, selection.clone()) {
        Ok(text) => text,
        Err(error) => {
            log::debug!("Falling back to source-based clipboard plain text: {error:?}");
            parsed
                .source
                .get(selection)
                .map(str::to_owned)
                .unwrap_or_default()
        }
    }
}

fn try_html_fragment_for_selection(
    parsed: &ParsedMarkdown,
    selection: Range<usize>,
    resolve_image_src: Option<&dyn Fn(&str) -> Option<String>>,
) -> Result<String, ClipboardHtmlError> {
    let nodes = build_tree(parsed.events())?;
    let mut serializer = ClipboardHtmlSerializer {
        parsed,
        selection,
        context: SerializeContext::default(),
        resolve_image_src,
    };
    let mut html = String::new();
    serializer.serialize_nodes(&nodes, &mut html)?;
    Ok(html)
}

fn try_plain_text_for_selection(
    parsed: &ParsedMarkdown,
    selection: Range<usize>,
) -> Result<String, ClipboardHtmlError> {
    let nodes = build_tree(parsed.events())?;
    let mut serializer = PlainTextSerializer {
        parsed,
        selection,
        output: String::new(),
        list_stack: Vec::new(),
        table_stack: Vec::new(),
    };
    serializer.serialize_root_nodes(&nodes)?;
    while serializer.output.ends_with('\n') {
        serializer.output.pop();
    }
    Ok(serializer.output)
}

pub(crate) fn basic_html_from_plain_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    for paragraph in text.split("\n\n") {
        if paragraph.is_empty() {
            continue;
        }

        html.push_str("<p>");
        for (index, line) in paragraph.split('\n').enumerate() {
            if index > 0 {
                html.push_str("<br>");
            }
            push_escaped_html(&mut html, line);
        }
        html.push_str("</p>");
    }

    if html.is_empty() {
        let mut fallback = String::from("<p>");
        push_escaped_html(&mut fallback, text);
        fallback.push_str("</p>");
        fallback
    } else {
        html
    }
}

#[derive(Debug)]
enum ClipboardHtmlError {
    UnbalancedMarkdownEvents,
    PartialHtmlBlock,
    MissingHtmlBlock,
    MissingMetadataBlock,
    PartialSubstitutedText,
    UnsupportedRawHtmlEvent,
}

#[derive(Clone, Debug)]
enum ClipboardNode {
    Container(ClipboardContainer),
    Leaf(ClipboardLeaf),
    Rule(Range<usize>),
}

#[derive(Clone, Debug)]
struct ClipboardContainer {
    tag: MarkdownTag,
    range: Range<usize>,
    children: Vec<ClipboardNode>,
}

#[derive(Clone, Debug)]
struct ClipboardLeaf {
    event: ClipboardLeafEvent,
    range: Range<usize>,
}

#[derive(Debug)]
struct ContainerBuilder {
    tag: MarkdownTag,
    range: Range<usize>,
    children: Vec<ClipboardNode>,
}

#[derive(Clone, Debug)]
enum ClipboardLeafEvent {
    Text,
    SubstitutedText(String),
    Code,
    SoftBreak,
    HardBreak,
    TaskListMarker(bool),
    FootnoteReference(SharedString),
    Html,
    InlineHtml,
}

#[derive(Default)]
struct SerializeContext {
    inside_table_head: bool,
}

struct ClipboardHtmlSerializer<'a> {
    parsed: &'a ParsedMarkdown,
    selection: Range<usize>,
    context: SerializeContext,
    resolve_image_src: Option<&'a dyn Fn(&str) -> Option<String>>,
}

#[derive(Clone, Copy)]
struct ListContext {
    ordered: bool,
    next_number: u64,
}

#[derive(Default)]
struct TableContext {
    is_first_cell: bool,
}

struct PlainTextSerializer<'a> {
    parsed: &'a ParsedMarkdown,
    selection: Range<usize>,
    output: String,
    list_stack: Vec<ListContext>,
    table_stack: Vec<TableContext>,
}

impl<'a> ClipboardHtmlSerializer<'a> {
    fn serialize_nodes(
        &mut self,
        nodes: &[ClipboardNode],
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        for node in nodes {
            self.serialize_node(node, html)?;
        }
        Ok(())
    }

    fn serialize_node(
        &mut self,
        node: &ClipboardNode,
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        let node_range = node.range();
        if !intersects(node_range, &self.selection) {
            return Ok(());
        }

        match node {
            ClipboardNode::Container(container) => self.serialize_container(container, html),
            ClipboardNode::Leaf(leaf) => self.serialize_leaf(leaf, html),
            ClipboardNode::Rule(_) => {
                html.push_str("<hr>");
                Ok(())
            }
        }
    }

    fn serialize_container(
        &mut self,
        container: &ClipboardContainer,
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        match &container.tag {
            MarkdownTag::Paragraph => {
                html.push_str("<p>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</p>");
            }
            MarkdownTag::Heading { level, .. } => {
                let tag = heading_tag(*level);
                html.push('<');
                html.push_str(tag);
                html.push('>');
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</");
                html.push_str(tag);
                html.push('>');
            }
            MarkdownTag::BlockQuote(_) => {
                html.push_str("<blockquote>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</blockquote>");
            }
            MarkdownTag::List(start_number) => {
                if let Some(start_number) = start_number {
                    html.push_str("<ol");
                    if *start_number != 1 {
                        push_attribute(html, "start", &start_number.to_string());
                    }
                    html.push('>');
                    self.serialize_nodes(&container.children, html)?;
                    html.push_str("</ol>");
                } else {
                    html.push_str("<ul>");
                    self.serialize_nodes(&container.children, html)?;
                    html.push_str("</ul>");
                }
            }
            MarkdownTag::Item => {
                html.push_str("<li>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</li>");
            }
            MarkdownTag::Table(_) => {
                self.serialize_markdown_table(container, html)?;
            }
            MarkdownTag::TableHead => {
                let was_inside_table_head = self.context.inside_table_head;
                self.context.inside_table_head = true;
                html.push_str("<thead>");
                let has_row_children = container.children.iter().any(|child| {
                    matches!(
                        child,
                        ClipboardNode::Container(ClipboardContainer {
                            tag: MarkdownTag::TableRow,
                            ..
                        })
                    )
                });
                if has_row_children {
                    self.serialize_nodes(&container.children, html)?;
                } else {
                    html.push_str("<tr>");
                    self.serialize_nodes(&container.children, html)?;
                    html.push_str("</tr>");
                }
                html.push_str("</thead>");
                self.context.inside_table_head = was_inside_table_head;
            }
            MarkdownTag::TableRow => {
                html.push_str("<tr>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</tr>");
            }
            MarkdownTag::TableCell => {
                let tag = if self.context.inside_table_head {
                    "th"
                } else {
                    "td"
                };
                html.push('<');
                html.push_str(tag);
                html.push('>');
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</");
                html.push_str(tag);
                html.push('>');
            }
            MarkdownTag::Emphasis => {
                html.push_str("<em>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</em>");
            }
            MarkdownTag::Strong => {
                html.push_str("<strong>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</strong>");
            }
            MarkdownTag::Strikethrough => {
                html.push_str("<del>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</del>");
            }
            MarkdownTag::Superscript => {
                html.push_str("<sup>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</sup>");
            }
            MarkdownTag::Subscript => {
                html.push_str("<sub>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</sub>");
            }
            MarkdownTag::Link { dest_url, .. } => {
                html.push_str("<a");
                push_attribute(html, "href", dest_url);
                html.push('>');
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</a>");
            }
            MarkdownTag::CodeBlock { .. } => {
                html.push_str("<pre><code>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</code></pre>");
            }
            MarkdownTag::MetadataBlock(_) => {
                self.serialize_metadata_block(container, html)?;
            }
            MarkdownTag::HtmlBlock => {
                if !contains(&self.selection, &container.range) {
                    // html_parser currently reuses block-wide source ranges for many descendants,
                    // so partial raw HTML selections cannot be cropped precisely yet.
                    return Err(ClipboardHtmlError::PartialHtmlBlock);
                }

                let Some(block) = self.parsed.html_blocks.get(&container.range.start) else {
                    return Err(ClipboardHtmlError::MissingHtmlBlock);
                };
                self.serialize_parsed_html_block(block, html);
            }
            MarkdownTag::DefinitionList => {
                html.push_str("<dl>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</dl>");
            }
            MarkdownTag::DefinitionListTitle => {
                html.push_str("<dt>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</dt>");
            }
            MarkdownTag::DefinitionListDefinition => {
                html.push_str("<dd>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</dd>");
            }
            MarkdownTag::Image { dest_url, .. } => {
                html.push_str("<img");
                push_attribute(html, "src", &self.resolve_image_src(dest_url));
                let alt = self.plain_text_for_nodes(&container.children)?;
                if !alt.is_empty() {
                    push_attribute(html, "alt", &alt);
                }
                html.push('>');
            }
            MarkdownTag::FootnoteDefinition(_) => {
                html.push_str("<div>");
                self.serialize_nodes(&container.children, html)?;
                html.push_str("</div>");
            }
        }
        Ok(())
    }

    fn serialize_markdown_table(
        &mut self,
        container: &ClipboardContainer,
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        html.push_str("<table>");

        let mut body_children = Vec::new();
        for child in &container.children {
            if matches!(
                child,
                ClipboardNode::Container(ClipboardContainer {
                    tag: MarkdownTag::TableHead,
                    ..
                })
            ) {
                self.serialize_node(child, html)?;
            } else {
                body_children.push(child);
            }
        }

        if !body_children.is_empty() {
            html.push_str("<tbody>");
            for child in body_children {
                self.serialize_node(child, html)?;
            }
            html.push_str("</tbody>");
        }

        html.push_str("</table>");
        Ok(())
    }

    fn serialize_leaf(
        &mut self,
        leaf: &ClipboardLeaf,
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        match &leaf.event {
            ClipboardLeafEvent::Text => {
                let intersection = intersection(&leaf.range, &self.selection).unwrap();
                push_escaped_html(html, &self.parsed.source[intersection]);
            }
            ClipboardLeafEvent::SubstitutedText(text) => {
                if !contains(&self.selection, &leaf.range) {
                    return Err(ClipboardHtmlError::PartialSubstitutedText);
                }
                push_escaped_html(html, text);
            }
            ClipboardLeafEvent::Code => {
                let intersection = intersection(&leaf.range, &self.selection).unwrap();
                html.push_str("<code>");
                push_escaped_html(html, &self.parsed.source[intersection]);
                html.push_str("</code>");
            }
            ClipboardLeafEvent::SoftBreak | ClipboardLeafEvent::HardBreak => {
                html.push_str("<br>");
            }
            ClipboardLeafEvent::TaskListMarker(checked) => {
                html.push_str(if *checked { "☑" } else { "☐" });
            }
            ClipboardLeafEvent::FootnoteReference(label) => {
                html.push('[');
                push_escaped_html(html, label);
                html.push(']');
            }
            ClipboardLeafEvent::Html | ClipboardLeafEvent::InlineHtml => {
                return Err(ClipboardHtmlError::UnsupportedRawHtmlEvent);
            }
        }
        Ok(())
    }

    fn plain_text_for_nodes(&self, nodes: &[ClipboardNode]) -> Result<String, ClipboardHtmlError> {
        let mut plain_text = String::new();
        for node in nodes {
            self.push_plain_text_for_node(node, &mut plain_text)?;
        }
        Ok(plain_text)
    }

    fn push_plain_text_for_node(
        &self,
        node: &ClipboardNode,
        plain_text: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        let node_range = node.range();
        if !intersects(node_range, &self.selection) {
            return Ok(());
        }

        match node {
            ClipboardNode::Container(container) => {
                for child in &container.children {
                    self.push_plain_text_for_node(child, plain_text)?;
                }
            }
            ClipboardNode::Leaf(leaf) => match &leaf.event {
                ClipboardLeafEvent::Text | ClipboardLeafEvent::Code => {
                    let intersection = intersection(&leaf.range, &self.selection).unwrap();
                    plain_text.push_str(&self.parsed.source[intersection]);
                }
                ClipboardLeafEvent::SubstitutedText(text) => {
                    if !contains(&self.selection, &leaf.range) {
                        return Err(ClipboardHtmlError::PartialSubstitutedText);
                    }
                    plain_text.push_str(text);
                }
                ClipboardLeafEvent::SoftBreak | ClipboardLeafEvent::HardBreak => {
                    plain_text.push('\n');
                }
                ClipboardLeafEvent::TaskListMarker(checked) => {
                    plain_text.push_str(if *checked { "☑" } else { "☐" });
                }
                ClipboardLeafEvent::FootnoteReference(label) => {
                    plain_text.push('[');
                    plain_text.push_str(label);
                    plain_text.push(']');
                }
                ClipboardLeafEvent::Html | ClipboardLeafEvent::InlineHtml => {
                    return Err(ClipboardHtmlError::UnsupportedRawHtmlEvent);
                }
            },
            ClipboardNode::Rule(_) => {}
        }

        Ok(())
    }

    fn serialize_metadata_block(
        &mut self,
        container: &ClipboardContainer,
        html: &mut String,
    ) -> Result<(), ClipboardHtmlError> {
        let Some(block) = self.parsed.metadata_blocks.get(&container.range.start) else {
            return Err(ClipboardHtmlError::MissingMetadataBlock);
        };

        if let Some(rows) = &block.rows {
            html.push_str("<table><tbody>");
            for row in rows {
                let row_range = union_range(&row.key, &row.value);
                if !intersects(&row_range, &self.selection) {
                    continue;
                }

                html.push_str("<tr><th>");
                if let Some(key_range) = intersection(&row.key, &self.selection) {
                    push_escaped_html(html, &self.parsed.source[key_range]);
                }
                html.push_str("</th><td>");
                if let Some(value_range) = intersection(&row.value, &self.selection) {
                    push_escaped_html(html, &self.parsed.source[value_range]);
                }
                html.push_str("</td></tr>");
            }
            html.push_str("</tbody></table>");
        } else {
            html.push_str("<pre><code>");
            if let Some(content_range) = intersection(&block.content_range, &self.selection) {
                push_escaped_html(html, &self.parsed.source[content_range]);
            }
            html.push_str("</code></pre>");
        }

        Ok(())
    }

    fn serialize_parsed_html_block(&self, block: &ParsedHtmlBlock, html: &mut String) {
        self.serialize_parsed_html_elements(&block.children, html);
    }

    fn serialize_parsed_html_elements(&self, elements: &[ParsedHtmlElement], html: &mut String) {
        for element in elements {
            self.serialize_parsed_html_element(element, html);
        }
    }

    fn serialize_parsed_html_element(&self, element: &ParsedHtmlElement, html: &mut String) {
        match element {
            ParsedHtmlElement::Heading(heading) => {
                let tag = heading_tag(heading.level);
                push_block_open_tag(html, tag, heading.text_align);
                self.serialize_parsed_html_paragraph(&heading.contents, html);
                push_close_tag(html, tag);
            }
            ParsedHtmlElement::List(list) => {
                html.push_str(if list.ordered { "<ol>" } else { "<ul>" });
                for item in &list.items {
                    html.push_str("<li>");
                    self.serialize_parsed_html_elements(&item.content, html);
                    html.push_str("</li>");
                }
                html.push_str(if list.ordered { "</ol>" } else { "</ul>" });
            }
            ParsedHtmlElement::Table(table) => self.serialize_parsed_html_table(table, html),
            ParsedHtmlElement::BlockQuote(block_quote) => {
                html.push_str("<blockquote>");
                self.serialize_parsed_html_elements(&block_quote.children, html);
                html.push_str("</blockquote>");
            }
            ParsedHtmlElement::Paragraph(paragraph) => {
                push_block_open_tag(html, "p", paragraph.text_align);
                self.serialize_parsed_html_paragraph(&paragraph.contents, html);
                html.push_str("</p>");
            }
            ParsedHtmlElement::Image(image) => self.serialize_parsed_html_image(image, html),
        }
    }

    fn serialize_parsed_html_table(&self, table: &ParsedHtmlTable, html: &mut String) {
        html.push_str("<table>");

        if let Some(caption) = &table.caption {
            html.push_str("<caption>");
            self.serialize_parsed_html_paragraph(caption, html);
            html.push_str("</caption>");
        }

        if !table.header.is_empty() {
            html.push_str("<thead>");
            self.serialize_parsed_html_rows(&table.header, html);
            html.push_str("</thead>");
        }

        if !table.body.is_empty() {
            html.push_str("<tbody>");
            self.serialize_parsed_html_rows(&table.body, html);
            html.push_str("</tbody>");
        }

        html.push_str("</table>");
    }

    fn serialize_parsed_html_rows(&self, rows: &[ParsedHtmlTableRow], html: &mut String) {
        for row in rows {
            html.push_str("<tr>");
            for column in &row.columns {
                self.serialize_parsed_html_column(column, html);
            }
            html.push_str("</tr>");
        }
    }

    fn serialize_parsed_html_column(&self, column: &ParsedHtmlTableColumn, html: &mut String) {
        let tag = if column.is_header { "th" } else { "td" };
        html.push('<');
        html.push_str(tag);
        if column.col_span > 1 {
            push_attribute(html, "colspan", &column.col_span.to_string());
        }
        if column.row_span > 1 {
            push_attribute(html, "rowspan", &column.row_span.to_string());
        }
        if let Some(alignment) = alignment_attr(column.alignment) {
            push_attribute(html, "align", alignment);
        }
        html.push('>');
        self.serialize_parsed_html_paragraph(&column.children, html);
        push_close_tag(html, tag);
    }

    fn serialize_parsed_html_paragraph(&self, paragraph: &HtmlParagraph, html: &mut String) {
        for chunk in paragraph {
            match chunk {
                HtmlParagraphChunk::Text(text) => self.serialize_parsed_html_text(text, html),
                HtmlParagraphChunk::Image(image) => self.serialize_parsed_html_image(image, html),
            }
        }
    }

    fn serialize_parsed_html_text(&self, text: &ParsedHtmlText, html: &mut String) {
        let mut boundaries = vec![0, text.contents.len()];
        for (range, _) in &text.highlights {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
        for (range, _) in &text.links {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        for window in boundaries.windows(2) {
            let segment = window[0]..window[1];
            if segment.is_empty() {
                continue;
            }

            let Some(text_segment) = text.contents.get(segment.clone()) else {
                continue;
            };
            if text_segment.is_empty() {
                continue;
            }

            let highlights = text
                .highlights
                .iter()
                .filter_map(|(range, style)| range_contains(range, &segment).then_some(style))
                .collect::<Vec<_>>();
            let link = text
                .links
                .iter()
                .find_map(|(range, url)| range_contains(range, &segment).then_some(url));

            if let Some(link) = link {
                html.push_str("<a");
                push_attribute(html, "href", link);
                html.push('>');
            }

            for highlight in &highlights {
                push_highlight_open_tag(html, highlight);
            }

            push_escaped_html(html, text_segment);

            for highlight in highlights.iter().rev() {
                push_highlight_close_tag(html, highlight);
            }

            if link.is_some() {
                html.push_str("</a>");
            }
        }
    }

    fn serialize_parsed_html_image(&self, image: &HtmlImage, html: &mut String) {
        html.push_str("<img");
        push_attribute(html, "src", &self.resolve_image_src(&image.dest_url));
        if let Some(alt_text) = &image.alt_text {
            push_attribute(html, "alt", alt_text);
        }
        html.push('>');
    }

    fn resolve_image_src(&self, original_src: &str) -> String {
        self.resolve_image_src
            .and_then(|resolver| resolver(original_src))
            .unwrap_or_else(|| original_src.to_string())
    }
}

impl<'a> PlainTextSerializer<'a> {
    fn serialize_root_nodes(&mut self, nodes: &[ClipboardNode]) -> Result<(), ClipboardHtmlError> {
        let mut wrote_root = false;
        for node in nodes {
            if !intersects(node.range(), &self.selection) {
                continue;
            }

            let start_len = self.output.len();
            self.serialize_node(node)?;
            let wrote_node = self.output.len() > start_len;
            if wrote_node {
                if wrote_root && !self.output.ends_with('\n') {
                    self.output.push('\n');
                }
                wrote_root = true;
            }
        }
        Ok(())
    }

    fn serialize_nodes(&mut self, nodes: &[ClipboardNode]) -> Result<(), ClipboardHtmlError> {
        for node in nodes {
            self.serialize_node(node)?;
        }
        Ok(())
    }

    fn serialize_node(&mut self, node: &ClipboardNode) -> Result<(), ClipboardHtmlError> {
        if !intersects(node.range(), &self.selection) {
            return Ok(());
        }

        match node {
            ClipboardNode::Container(container) => self.serialize_container(container),
            ClipboardNode::Leaf(leaf) => self.serialize_leaf(leaf),
            ClipboardNode::Rule(_) => {
                self.write_inline("---");
                self.ensure_newline();
                Ok(())
            }
        }
    }

    fn serialize_container(
        &mut self,
        container: &ClipboardContainer,
    ) -> Result<(), ClipboardHtmlError> {
        match &container.tag {
            MarkdownTag::Paragraph
            | MarkdownTag::Heading { .. }
            | MarkdownTag::BlockQuote(_)
            | MarkdownTag::FootnoteDefinition(_) => {
                self.serialize_nodes(&container.children)?;
                self.ensure_newline();
            }
            MarkdownTag::List(start_number) => {
                self.ensure_newline_if_needed();
                self.list_stack.push(ListContext {
                    ordered: start_number.is_some(),
                    next_number: start_number.unwrap_or(1),
                });
                self.serialize_nodes(&container.children)?;
                self.list_stack.pop();
            }
            MarkdownTag::Item => {
                self.serialize_list_item(container)?;
            }
            MarkdownTag::Table(_) => {
                self.serialize_nodes(&container.children)?;
                self.ensure_newline_if_needed();
            }
            MarkdownTag::TableHead => {
                let has_row_children = container.children.iter().any(|child| {
                    matches!(
                        child,
                        ClipboardNode::Container(ClipboardContainer {
                            tag: MarkdownTag::TableRow,
                            ..
                        })
                    )
                });
                if has_row_children {
                    self.serialize_nodes(&container.children)?;
                } else {
                    self.table_stack.push(TableContext {
                        is_first_cell: true,
                    });
                    self.serialize_nodes(&container.children)?;
                    self.table_stack.pop();
                    self.ensure_newline();
                }
            }
            MarkdownTag::TableRow => {
                self.table_stack.push(TableContext {
                    is_first_cell: true,
                });
                self.serialize_nodes(&container.children)?;
                self.table_stack.pop();
                self.ensure_newline();
            }
            MarkdownTag::TableCell => {
                if let Some(table_context) = self.table_stack.last_mut() {
                    if table_context.is_first_cell {
                        table_context.is_first_cell = false;
                    } else {
                        self.output.push('\t');
                    }
                }
                self.serialize_nodes(&container.children)?;
            }
            MarkdownTag::Emphasis
            | MarkdownTag::Strong
            | MarkdownTag::Strikethrough
            | MarkdownTag::Superscript
            | MarkdownTag::Subscript
            | MarkdownTag::Link { .. }
            | MarkdownTag::DefinitionList => {
                self.serialize_nodes(&container.children)?;
            }
            MarkdownTag::CodeBlock { .. } => {
                self.serialize_nodes(&container.children)?;
                self.ensure_newline_if_needed();
            }
            MarkdownTag::MetadataBlock(_) => {
                self.serialize_metadata_block_plain_text(container)?;
            }
            MarkdownTag::HtmlBlock => {
                if !contains(&self.selection, &container.range) {
                    return Err(ClipboardHtmlError::PartialHtmlBlock);
                }
                let Some(block) = self.parsed.html_blocks.get(&container.range.start) else {
                    return Err(ClipboardHtmlError::MissingHtmlBlock);
                };
                self.serialize_parsed_html_block_plain_text(block);
            }
            MarkdownTag::DefinitionListTitle | MarkdownTag::DefinitionListDefinition => {
                self.serialize_nodes(&container.children)?;
                self.ensure_newline();
            }
            MarkdownTag::Image { .. } => {
                let alt_text = self.plain_text_for_nodes(&container.children)?;
                self.write_inline(&alt_text);
            }
        }
        Ok(())
    }

    fn serialize_list_item(
        &mut self,
        container: &ClipboardContainer,
    ) -> Result<(), ClipboardHtmlError> {
        self.ensure_newline_if_needed();
        let has_task_marker = item_starts_with_task_marker(&container.children);
        if !has_task_marker {
            let depth = self.list_stack.len().saturating_sub(1);
            self.output.push_str(&"  ".repeat(depth));
            if let Some(list_context) = self.list_stack.last_mut() {
                if list_context.ordered {
                    self.output
                        .push_str(&format!("{}. ", list_context.next_number));
                    list_context.next_number += 1;
                } else {
                    self.output.push_str("• ");
                }
            }
        }

        let mut wrote_content = false;
        for child in &container.children {
            if !intersects(child.range(), &self.selection) {
                continue;
            }

            if matches!(
                child,
                ClipboardNode::Container(ClipboardContainer {
                    tag: MarkdownTag::List(_),
                    ..
                })
            ) && wrote_content
                && !self.output.ends_with('\n')
            {
                self.output.push('\n');
            }

            let start_len = self.output.len();
            self.serialize_node(child)?;
            wrote_content |= self.output.len() > start_len;
        }
        self.ensure_newline();
        Ok(())
    }

    fn serialize_leaf(&mut self, leaf: &ClipboardLeaf) -> Result<(), ClipboardHtmlError> {
        match &leaf.event {
            ClipboardLeafEvent::Text | ClipboardLeafEvent::Code => {
                if let Some(intersection) = intersection(&leaf.range, &self.selection) {
                    self.write_inline(&self.parsed.source[intersection]);
                }
            }
            ClipboardLeafEvent::SubstitutedText(text) => {
                if !contains(&self.selection, &leaf.range) {
                    return Err(ClipboardHtmlError::PartialSubstitutedText);
                }
                self.write_inline(text);
            }
            ClipboardLeafEvent::SoftBreak => self.write_inline(" "),
            ClipboardLeafEvent::HardBreak => self.ensure_newline(),
            ClipboardLeafEvent::TaskListMarker(checked) => {
                self.write_inline(if *checked { "☑ " } else { "☐ " });
            }
            ClipboardLeafEvent::FootnoteReference(label) => {
                self.write_inline("[");
                self.write_inline(label);
                self.write_inline("]");
            }
            ClipboardLeafEvent::Html | ClipboardLeafEvent::InlineHtml => {
                if let Some(intersection) = intersection(&leaf.range, &self.selection) {
                    self.write_inline(&self.parsed.source[intersection]);
                }
            }
        }
        Ok(())
    }

    fn serialize_metadata_block_plain_text(
        &mut self,
        container: &ClipboardContainer,
    ) -> Result<(), ClipboardHtmlError> {
        let Some(block) = self.parsed.metadata_blocks.get(&container.range.start) else {
            return Err(ClipboardHtmlError::MissingMetadataBlock);
        };

        if let Some(rows) = &block.rows {
            for row in rows {
                let row_range = union_range(&row.key, &row.value);
                if !intersects(&row_range, &self.selection) {
                    continue;
                }
                if let Some(key_range) = intersection(&row.key, &self.selection) {
                    self.write_inline(&self.parsed.source[key_range]);
                }
                self.output.push('\t');
                if let Some(value_range) = intersection(&row.value, &self.selection) {
                    self.write_inline(&self.parsed.source[value_range]);
                }
                self.ensure_newline();
            }
        } else if let Some(content_range) = intersection(&block.content_range, &self.selection) {
            self.write_inline(&self.parsed.source[content_range]);
            self.ensure_newline_if_needed();
        }

        Ok(())
    }

    fn serialize_parsed_html_block_plain_text(&mut self, block: &ParsedHtmlBlock) {
        self.serialize_parsed_html_elements_plain_text(&block.children);
        self.ensure_newline_if_needed();
    }

    fn serialize_parsed_html_elements_plain_text(&mut self, elements: &[ParsedHtmlElement]) {
        for element in elements {
            match element {
                ParsedHtmlElement::Heading(heading) => {
                    self.serialize_parsed_html_paragraph_plain_text(&heading.contents);
                    self.ensure_newline();
                }
                ParsedHtmlElement::List(list) => {
                    self.list_stack.push(ListContext {
                        ordered: list.ordered,
                        next_number: 1,
                    });
                    for item in &list.items {
                        self.ensure_newline_if_needed();
                        let depth = self.list_stack.len().saturating_sub(1);
                        self.output.push_str(&"  ".repeat(depth));
                        if let Some(list_context) = self.list_stack.last_mut() {
                            if list_context.ordered {
                                self.output
                                    .push_str(&format!("{}. ", list_context.next_number));
                                list_context.next_number += 1;
                            } else {
                                self.output.push_str("• ");
                            }
                        }
                        self.serialize_parsed_html_elements_plain_text(&item.content);
                        self.ensure_newline();
                    }
                    self.list_stack.pop();
                }
                ParsedHtmlElement::Table(table) => {
                    self.serialize_parsed_html_rows_plain_text(&table.header);
                    self.serialize_parsed_html_rows_plain_text(&table.body);
                    self.ensure_newline_if_needed();
                }
                ParsedHtmlElement::BlockQuote(block_quote) => {
                    self.serialize_parsed_html_elements_plain_text(&block_quote.children);
                    self.ensure_newline_if_needed();
                }
                ParsedHtmlElement::Paragraph(paragraph) => {
                    self.serialize_parsed_html_paragraph_plain_text(&paragraph.contents);
                    self.ensure_newline();
                }
                ParsedHtmlElement::Image(image) => {
                    if let Some(alt_text) = &image.alt_text {
                        self.write_inline(alt_text);
                    }
                }
            }
        }
    }

    fn serialize_parsed_html_rows_plain_text(&mut self, rows: &[ParsedHtmlTableRow]) {
        for row in rows {
            let mut first_cell = true;
            for column in &row.columns {
                if first_cell {
                    first_cell = false;
                } else {
                    self.output.push('\t');
                }
                self.serialize_parsed_html_paragraph_plain_text(&column.children);
            }
            self.ensure_newline();
        }
    }

    fn serialize_parsed_html_paragraph_plain_text(&mut self, paragraph: &HtmlParagraph) {
        for chunk in paragraph {
            match chunk {
                HtmlParagraphChunk::Text(text) => self.write_inline(&text.contents),
                HtmlParagraphChunk::Image(image) => {
                    if let Some(alt_text) = &image.alt_text {
                        self.write_inline(alt_text);
                    }
                }
            }
        }
    }

    fn plain_text_for_nodes(&self, nodes: &[ClipboardNode]) -> Result<String, ClipboardHtmlError> {
        let mut serializer = PlainTextSerializer {
            parsed: self.parsed,
            selection: self.selection.clone(),
            output: String::new(),
            list_stack: self.list_stack.clone(),
            table_stack: Vec::new(),
        };
        serializer.serialize_nodes(nodes)?;
        while serializer.output.ends_with('\n') {
            serializer.output.pop();
        }
        Ok(serializer.output)
    }

    fn write_inline(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn ensure_newline_if_needed(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn ensure_newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }
}

impl ClipboardNode {
    fn range(&self) -> &Range<usize> {
        match self {
            ClipboardNode::Container(container) => &container.range,
            ClipboardNode::Leaf(leaf) => &leaf.range,
            ClipboardNode::Rule(range) => range,
        }
    }
}

fn build_tree(
    events: &[(Range<usize>, MarkdownEvent)],
) -> Result<Vec<ClipboardNode>, ClipboardHtmlError> {
    let mut roots = Vec::new();
    let mut stack: Vec<ContainerBuilder> = Vec::new();

    for (range, event) in events {
        match event {
            MarkdownEvent::Start(tag) => {
                stack.push(ContainerBuilder {
                    tag: tag.clone(),
                    range: range.clone(),
                    children: Vec::new(),
                });
            }
            MarkdownEvent::End(_) => {
                let Some(mut builder) = stack.pop() else {
                    return Err(ClipboardHtmlError::UnbalancedMarkdownEvents);
                };
                builder.range = union_range(&builder.range, range);
                let node = ClipboardNode::Container(ClipboardContainer {
                    tag: builder.tag,
                    range: builder.range,
                    children: builder.children,
                });
                push_node(node, &mut stack, &mut roots);
            }
            MarkdownEvent::Text => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::Text,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::SubstitutedText(text) => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::SubstitutedText(text.clone()),
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::Code => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::Code,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::SoftBreak => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::SoftBreak,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::HardBreak => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::HardBreak,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::TaskListMarker(checked) => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::TaskListMarker(*checked),
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::FootnoteReference(label) => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::FootnoteReference(label.clone()),
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::Html => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::Html,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::InlineHtml => push_node(
                ClipboardNode::Leaf(ClipboardLeaf {
                    event: ClipboardLeafEvent::InlineHtml,
                    range: range.clone(),
                }),
                &mut stack,
                &mut roots,
            ),
            MarkdownEvent::Rule => {
                push_node(ClipboardNode::Rule(range.clone()), &mut stack, &mut roots);
            }
            MarkdownEvent::RootStart | MarkdownEvent::RootEnd(_) => {}
        }
    }

    if !stack.is_empty() {
        return Err(ClipboardHtmlError::UnbalancedMarkdownEvents);
    }

    Ok(roots)
}

fn push_node(
    node: ClipboardNode,
    stack: &mut Vec<ContainerBuilder>,
    roots: &mut Vec<ClipboardNode>,
) {
    if let Some(parent) = stack.last_mut() {
        parent.range = union_range(&parent.range, node.range());
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

fn alignment_attr(alignment: Alignment) -> Option<&'static str> {
    match alignment {
        Alignment::Left => Some("left"),
        Alignment::Center => Some("center"),
        Alignment::Right => Some("right"),
        Alignment::None => None,
    }
}

fn push_block_open_tag(html: &mut String, tag: &str, text_align: Option<TextAlign>) {
    html.push('<');
    html.push_str(tag);
    if let Some(text_align) = text_align {
        let value = match text_align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        };
        push_attribute(html, "style", &format!("text-align:{value}"));
    }
    html.push('>');
}

fn push_close_tag(html: &mut String, tag: &str) {
    html.push_str("</");
    html.push_str(tag);
    html.push('>');
}

fn push_highlight_open_tag(html: &mut String, highlight: &HtmlHighlightStyle) {
    if highlight.link {
        return;
    }
    if highlight.italic || highlight.oblique {
        html.push_str("<em>");
    }
    if highlight.weight >= FontWeight::BOLD {
        html.push_str("<strong>");
    }
    if highlight.strikethrough {
        html.push_str("<del>");
    }
    if highlight.underline {
        html.push_str("<u>");
    }
}

fn push_highlight_close_tag(html: &mut String, highlight: &HtmlHighlightStyle) {
    if highlight.link {
        return;
    }
    if highlight.underline {
        html.push_str("</u>");
    }
    if highlight.strikethrough {
        html.push_str("</del>");
    }
    if highlight.weight >= FontWeight::BOLD {
        html.push_str("</strong>");
    }
    if highlight.italic || highlight.oblique {
        html.push_str("</em>");
    }
}

fn push_attribute(html: &mut String, name: &str, value: &str) {
    html.push(' ');
    html.push_str(name);
    html.push_str("=\"");
    push_escaped_html_attribute(html, value);
    html.push('"');
}

fn push_escaped_html(html: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            _ => html.push(character),
        }
    }
}

fn push_escaped_html_attribute(html: &mut String, text: &str) {
    push_escaped_html(html, text);
}

fn item_starts_with_task_marker(children: &[ClipboardNode]) -> bool {
    children.iter().any(|child| match child {
        ClipboardNode::Leaf(ClipboardLeaf {
            event: ClipboardLeafEvent::TaskListMarker(_),
            ..
        }) => true,
        ClipboardNode::Leaf(_) => false,
        ClipboardNode::Container(container) => item_starts_with_task_marker(&container.children),
        ClipboardNode::Rule(_) => false,
    })
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

fn intersects(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn intersection(left: &Range<usize>, right: &Range<usize>) -> Option<Range<usize>> {
    let start = left.start.max(right.start);
    let end = left.end.min(right.end);
    (start < end).then_some(start..end)
}

fn range_contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

fn union_range(left: &Range<usize>, right: &Range<usize>) -> Range<usize> {
    left.start.min(right.start)..left.end.max(right.end)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ops::Range;
    use std::sync::Arc;

    use gpui::SharedString;
    use sum_tree::TreeMap;

    use crate::ParsedMarkdown;
    use crate::parser::parse_markdown_with_options;

    use super::{
        basic_html_from_plain_text, html_fragment_for_selection, plain_text_for_selection,
    };

    fn parse_markdown(
        source: &str,
        parse_html: bool,
        parse_metadata_blocks: bool,
    ) -> ParsedMarkdown {
        let parsed = parse_markdown_with_options(source, parse_html, false, parse_metadata_blocks);
        ParsedMarkdown {
            source: SharedString::from(source.to_string()),
            events: Arc::from(parsed.events),
            languages_by_name: TreeMap::default(),
            languages_by_path: TreeMap::default(),
            root_block_starts: Arc::from(parsed.root_block_starts),
            html_blocks: parsed.html_blocks,
            metadata_blocks: parsed.metadata_blocks,
            mermaid_diagrams: BTreeMap::default(),
            heading_slugs: parsed.heading_slugs,
            footnote_definitions: parsed.footnote_definitions,
        }
    }

    fn html_for_selection(
        source: &str,
        selection: Range<usize>,
        plain_text: &str,
        parse_html: bool,
        parse_metadata_blocks: bool,
    ) -> String {
        html_for_selection_with_image_resolver(
            source,
            selection,
            plain_text,
            parse_html,
            parse_metadata_blocks,
            None,
        )
    }

    fn html_for_selection_with_image_resolver(
        source: &str,
        selection: Range<usize>,
        plain_text: &str,
        parse_html: bool,
        parse_metadata_blocks: bool,
        resolve_image_src: Option<&dyn Fn(&str) -> Option<String>>,
    ) -> String {
        let parsed = parse_markdown(source, parse_html, parse_metadata_blocks);
        html_fragment_for_selection(&parsed, selection, plain_text, resolve_image_src)
    }

    #[test]
    fn basic_plain_text_fallback_preserves_paragraphs_and_line_breaks() {
        assert_eq!(
            basic_html_from_plain_text("a < b\nc\n\nd & e"),
            "<p>a &lt; b<br>c</p><p>d &amp; e</p>"
        );
    }

    #[test]
    fn serializes_single_paragraph_plain_text() {
        assert_eq!(
            html_for_selection("hello", 0..5, "unused", false, false),
            "<p>hello</p>"
        );
        assert_eq!(
            plain_text_for_selection(&parse_markdown("hello", false, false), 0..5),
            "hello"
        );
    }

    #[test]
    fn serializes_bold_text() {
        assert_eq!(
            html_for_selection("**bold**", 2..6, "unused", false, false),
            "<p><strong>bold</strong></p>"
        );
    }

    #[test]
    fn serializes_italic_text() {
        assert_eq!(
            html_for_selection("*italic*", 1..7, "unused", false, false),
            "<p><em>italic</em></p>"
        );
    }

    #[test]
    fn serializes_inline_code() {
        assert_eq!(
            html_for_selection("use `code` here", 5..9, "unused", false, false),
            "<p><code>code</code></p>"
        );
    }

    #[test]
    fn serializes_complete_link() {
        assert_eq!(
            html_for_selection("[link](https://example.com)", 1..5, "unused", false, false),
            "<p><a href=\"https://example.com\">link</a></p>"
        );
    }

    #[test]
    fn serializes_partial_link_selection() {
        assert_eq!(
            html_for_selection("[link](https://example.com)", 2..4, "unused", false, false),
            "<p><a href=\"https://example.com\">in</a></p>"
        );
    }

    #[test]
    fn serializes_markdown_image_with_resolved_src_override() {
        let markdown = "![Alt](./image.png)";
        let resolve_image_src =
            |src: &str| (src == "./image.png").then(|| "data:image/png;base64,abc123".to_string());
        assert_eq!(
            html_for_selection_with_image_resolver(
                markdown,
                0..markdown.len(),
                "unused",
                false,
                false,
                Some(&resolve_image_src),
            ),
            "<p><img src=\"data:image/png;base64,abc123\" alt=\"Alt\"></p>"
        );
    }

    #[test]
    fn serializes_raw_html_image_with_resolved_src_override() {
        let resolve_image_src =
            |src: &str| (src == "./image.svg").then(|| "data:image/png;base64,xyz".to_string());
        let markdown = "<img src=\"./image.svg\" alt=\"Diagram\">";
        assert_eq!(
            html_for_selection_with_image_resolver(
                markdown,
                0..markdown.len(),
                "unused",
                true,
                false,
                Some(&resolve_image_src),
            ),
            "<img src=\"data:image/png;base64,xyz\" alt=\"Diagram\">"
        );
    }

    #[test]
    fn serializes_multiple_paragraphs() {
        assert_eq!(
            html_for_selection("one\n\ntwo", 0..8, "unused", false, false),
            "<p>one</p><p>two</p>"
        );
    }

    #[test]
    fn serializes_heading_and_paragraph() {
        assert_eq!(
            html_for_selection("# Title\n\nBody", 2..13, "unused", false, false),
            "<h1>Title</h1><p>Body</p>"
        );
    }

    #[test]
    fn serializes_lists() {
        assert_eq!(
            html_for_selection("- one\n- two", 2..11, "unused", false, false),
            "<ul><li>one</li><li>two</li></ul>"
        );
    }

    #[test]
    fn serializes_nested_lists() {
        let markdown = "- parent\n  - child\n1. next";
        assert_eq!(
            html_for_selection(markdown, 2..26, "unused", false, false),
            "<ul><li>parent<ul><li>child</li></ul></li></ul><ol><li>next</li></ol>"
        );
    }

    #[test]
    fn serializes_blockquote() {
        assert_eq!(
            html_for_selection("> quote", 2..7, "unused", false, false),
            "<blockquote><p>quote</p></blockquote>"
        );
    }

    #[test]
    fn serializes_task_list() {
        assert_eq!(
            html_for_selection("- [x] done\n- [ ] todo", 2..21, "unused", false, false),
            "<ul><li>☑done</li><li>☐todo</li></ul>"
        );
    }

    #[test]
    fn serializes_table() {
        let markdown = "| a | b |\n|---|---|\n| c | d |";
        assert_eq!(
            html_for_selection(markdown, 2..31, "unused", false, false),
            "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>c</td><td>d</td></tr></tbody></table>"
        );
    }

    #[test]
    fn serializes_code_block_for_full_and_partial_selection() {
        let markdown = "```rs\nlet x = 1;\nlet y = 2;\n```";
        assert_eq!(
            html_for_selection(markdown, 6..28, "unused", false, false),
            "<pre><code>let x = 1;\nlet y = 2;\n</code></pre>"
        );
        assert_eq!(
            html_for_selection(markdown, 6..16, "unused", false, false),
            "<pre><code>let x = 1;</code></pre>"
        );
    }

    #[test]
    fn serializes_metadata_block_rows_as_table() {
        let markdown = "---\ntitle: Post\nauthor: Zed\n---";
        assert_eq!(
            html_for_selection(markdown, 4..27, "unused", false, true),
            "<table><tbody><tr><th>title</th><td>Post</td></tr><tr><th>author</th><td>Zed</td></tr></tbody></table>"
        );
    }

    #[test]
    fn serializes_metadata_block_without_rows_as_code() {
        let markdown = "---\ntags:\n  - zed\n---";
        assert_eq!(
            html_for_selection(markdown, 4..17, "unused", false, true),
            "<pre><code>tags:\n  - zed</code></pre>"
        );
    }

    #[test]
    fn serializes_smart_punctuation() {
        assert_eq!(
            html_for_selection("--", 0..2, "unused", false, false),
            "<p>–</p>"
        );
    }

    #[test]
    fn serializes_raw_html_block_when_fully_selected() {
        let markdown = "<p>Some <strong>bold</strong> <a href=\"https://example.com\">link</a></p>";
        assert_eq!(
            html_for_selection(markdown, 0..markdown.len(), "unused", true, false),
            "<p>Some <strong>bold</strong> <a href=\"https://example.com\">link</a></p>"
        );
    }

    #[test]
    fn falls_back_for_partial_raw_html_block_selection() {
        let markdown = "<blockquote><p>hello</p></blockquote>";
        assert_eq!(
            html_for_selection(markdown, 13..18, "hello", true, false),
            "<p>hello</p>"
        );
    }

    #[test]
    fn serializes_across_multiple_root_blocks() {
        assert_eq!(
            html_for_selection("# Title\n\n- a\n- b", 2..16, "unused", false, false),
            "<h1>Title</h1><ul><li>a</li><li>b</li></ul>"
        );
    }

    #[test]
    fn plain_text_for_selection_matches_rendered_structure() {
        let markdown = "# Title\n\n- [x] one\n- two\n\n| a | b |\n|---|---|\n| c | d |";
        let parsed = parse_markdown(markdown, false, false);

        assert_eq!(
            plain_text_for_selection(&parsed, 0..markdown.len()),
            "Title\n☑ one\n• two\na\tb\nc\td"
        );
    }
}
