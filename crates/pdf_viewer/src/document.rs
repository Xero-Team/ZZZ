use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Context as _;
use gpui::RenderImage;
use image::{Frame, RgbaImage};
use smallvec::SmallVec;
use zpdf::{
    AnnotationSpec, ContentInterpreter, FieldKind, FormFiller, IccCache, ImageCache,
    IncrementalWriter, InterpretedPage, MarkupKind, ObjectId, OutlineItem as ZpdfOutlineItem,
    PageInterpretOptions, PdfDocument, PdfObject, Rect, RenderBackend, TextSpan,
    interpret_page_with_cache, spans_to_text,
};

use crate::edit::{MarkupSource, PdfEdit};

/// A parsed PDF document plus precomputed summary information. This type owns
/// the underlying [`zpdf::PdfDocument`], which is neither `Send` nor `Sync`, so
/// it must live on a single dedicated thread (see `worker.rs`).
pub struct LoadedPdfDocument {
    pub summary: PdfDocumentSummary,
    /// Absolute path the document was opened from, used for error messages and
    /// for reopening this thread's copy after an edit.
    path: PathBuf,
    /// The bytes this document was parsed from. Kept so an edit can hand the
    /// current file to zpdf's incremental writer without a second read.
    data: Arc<[u8]>,
    /// The password the document was opened with, reused to encrypt objects an
    /// incremental update appends. Zeroed on drop.
    password: Vec<u8>,
    pdf: PdfDocument,
    /// Reused across pages so an embedded ICC profile is transformed and cached
    /// once per document rather than once per page.
    colors: IccCache,
    /// Interpreted pages, kept so a zoom change only has to rasterize again.
    interpreted: InterpretedPages,
}

impl Drop for LoadedPdfDocument {
    fn drop(&mut self) {
        self.password.fill(0);
    }
}

#[derive(Debug, Clone)]
pub struct PdfDocumentSummary {
    pub version: (u8, u8),
    pub page_count: usize,
    pub pages: Vec<PdfPageSummary>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub outline: Vec<OutlineItem>,
    pub security: PdfSecurity,
    /// Display label per page from the document's number tree, falling back to
    /// the 1-based page number so callers never have to special-case its
    /// absence.
    pub page_labels: Vec<String>,
    /// Interactive-form widgets with the page each one appears on, so the view
    /// can overlay them without another round trip.
    pub form_widgets: Vec<FormWidgetInfo>,
}

#[derive(Debug, Clone)]
pub struct PdfPageSummary {
    /// Page width in PDF points after accounting for rotation.
    pub width: f64,
    /// Page height in PDF points after accounting for rotation.
    pub height: f64,
    /// Unrotated width, needed to invert the rotation when mapping between the
    /// rendered bitmap and default page user space.
    pub user_width: f64,
    /// Unrotated height, see [`Self::user_width`].
    pub user_height: f64,
    /// Effective-box origin in default user space; annotation and widget
    /// rectangles are absolute and must be offset by this before display.
    pub origin_x: f64,
    pub origin_y: f64,
    /// The page's `/Rotate`, normalized to `0..360`.
    pub rotate: i32,
}

/// A single entry in the document outline (bookmarks). `page_index` is resolved
/// from the destination when possible.
#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub title: String,
    pub page_index: Option<usize>,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfSecurity {
    Unencrypted,
    Encrypted,
}

/// The interactive-form control a widget presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormWidgetKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    Signature,
    /// A push button or an unrecognized field type; shown but not editable.
    Other,
}

/// One widget of an interactive-form field, positioned on a page.
///
/// A field with separate widgets (a radio group, a field repeated in a header
/// and footer) yields one entry per widget, all sharing `name`.
#[derive(Debug, Clone)]
pub struct FormWidgetInfo {
    pub page: usize,
    pub name: String,
    pub kind: FormWidgetKind,
    /// Widget rectangle in default page user space.
    pub rect: (f64, f64, f64, f64),
    /// The value a renderer draws, or `None` when nothing is shown.
    pub value: Option<String>,
    pub readonly: bool,
    pub multiline: bool,
    pub password: bool,
    /// Display labels for a choice field's options.
    pub options: Vec<String>,
    /// The button's on-state name (`/AP /N` key other than `Off`), which is the
    /// value a filler must store to check it.
    pub on_state: Option<String>,
}

/// An annotation read from a page, with the object id needed to edit it.
#[derive(Debug, Clone)]
pub struct PageAnnotation {
    pub id: ObjectId,
    pub page: usize,
    /// `/Subtype`, e.g. `Highlight`, `Link`, `Widget`, `Text`.
    pub subtype: String,
    /// Annotation rectangle in default page user space.
    pub rect: (f64, f64, f64, f64),
    pub contents: Option<String>,
    /// `/T` — the annotation's author. Widgets carry the field name here.
    pub author: Option<String>,
    /// In-document link destination, resolved through zpdf's destination parser.
    pub dest_page: Option<usize>,
    /// External link target (`/A /S /URI`).
    pub uri: Option<String>,
    /// `/F` flag bits; bit 2 (`NoView`) and bit 1 (`Hidden`) suppress display.
    pub flags: i64,
}

impl PageAnnotation {
    /// Whether this annotation should be drawn on screen.
    pub fn is_visible(&self) -> bool {
        self.flags & (ANNOT_FLAG_HIDDEN | ANNOT_FLAG_NOVIEW) == 0
            && self.rect.2 > self.rect.0
            && self.rect.3 > self.rect.1
    }

    /// Whether this annotation is a `Widget` owned by the interactive form
    /// rather than a document annotation the reader would edit.
    pub fn is_form_widget(&self) -> bool {
        self.subtype == "Widget" || self.subtype == "Sig"
    }
}

/// Annotation flag bit 2: hidden.
const ANNOT_FLAG_HIDDEN: i64 = 1 << 1;
/// Annotation flag bit 6: never render on screen.
const ANNOT_FLAG_NOVIEW: i64 = 1 << 5;
/// Field flag bit 1: read-only (ISO 32000-1 Table 227).
const FIELD_FLAG_READONLY: i64 = 1 << 0;
/// Field flag bit 16: radio button group (Table 227).
const FIELD_FLAG_RADIO: i64 = 1 << 15;

/// A signature field and the result of checking it.
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub field_name: String,
    /// `/M` — the signing time as the raw PDF date string.
    pub signing_time: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// What `/ByteRange` covers, in words, and whether the digest matched.
    pub coverage: String,
    pub digest: String,
    pub crypto: String,
}

/// An attached file carried in the document's embedded-file name tree.
#[derive(Debug, Clone)]
pub struct AttachmentInfo {
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
}

/// Document-level details that are too costly or too rarely used to compute
/// while opening, so the viewer asks for them on demand.
#[derive(Debug, Clone, Default)]
pub struct PdfDocumentDetails {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub mod_date: Option<String>,
    pub xmp: Option<XmpSummary>,
    pub signatures: Vec<SignatureInfo>,
    pub attachments: Vec<AttachmentInfo>,
    /// Output intent descriptions, e.g. `GTS_PDFA1` with an ICC profile name.
    pub output_intents: Vec<String>,
    pub has_optional_content: bool,
    pub is_tagged: bool,
}

/// The subset of the document's XMP packet the properties panel shows.
#[derive(Debug, Clone, Default)]
pub struct XmpSummary {
    pub title: Option<String>,
    pub creators: Vec<String>,
    pub description: Option<String>,
    pub subjects: Vec<String>,
    pub producer: Option<String>,
    pub creator_tool: Option<String>,
    pub modify_date: Option<String>,
}

/// A rendered page bitmap ready to be displayed by GPUI. Cheaply cloneable.
#[derive(Clone)]
pub struct PagePreview {
    pub image: Arc<RenderImage>,
    /// The scale (dpi / 72) the page was rendered at, so callers can detect
    /// when a cached render no longer matches the requested zoom.
    pub scale: f32,
    pub blank_render_suspected: bool,
}

impl PagePreview {
    /// Bytes the decoded bitmap occupies, for the cache memory budget. The
    /// image is RGBA, so this is `width * height * 4` device pixels.
    pub fn bytes(&self) -> u64 {
        let size = self.image.size(0);
        let width = u32::from(size.width) as u64;
        let height = u32::from(size.height) as u64;
        width.saturating_mul(height).saturating_mul(4)
    }
}

/// Mapping a document's outline onto the viewer's flat, depth-tagged list stops
/// at these bounds so a hostile or malformed outline tree cannot exhaust memory.
const MAX_OUTLINE_ITEMS: usize = 4096;
const MAX_OUTLINE_DEPTH: usize = 32;

/// How many interpreted pages stay resident. Each entry pins that page's
/// display list together with the fonts and decoded images it refers to, so this
/// is a count bound rather than a byte budget: a dozen pages covers the visible
/// window plus prefetch, and keeps a zoom change down to a re-raster instead of
/// a fresh interpretation of every visible page.
const INTERPRETED_PAGE_LIMIT: usize = 12;

/// A small least-recently-used set of interpreted pages.
#[derive(Default)]
struct InterpretedPages {
    entries: HashMap<usize, Arc<InterpretedPage>>,
    /// Page indices by recency, oldest first.
    recency: VecDeque<usize>,
}

impl InterpretedPages {
    fn get(&mut self, index: usize) -> Option<Arc<InterpretedPage>> {
        let page = self.entries.get(&index)?.clone();
        self.recency.retain(|&cached| cached != index);
        self.recency.push_back(index);
        Some(page)
    }

    fn insert(&mut self, index: usize, page: Arc<InterpretedPage>) {
        self.recency.retain(|&cached| cached != index);
        self.recency.push_back(index);
        self.entries.insert(index, page);
        while self.entries.len() > INTERPRETED_PAGE_LIMIT {
            let Some(oldest) = self.recency.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

impl LoadedPdfDocument {
    /// Open a document from shared bytes. The `Arc<[u8]>` is handed straight to
    /// zpdf, so reopening the same buffer on several worker threads shares the
    /// underlying bytes and only duplicates the per-thread parse caches.
    pub fn open(path: PathBuf, data: Arc<[u8]>) -> anyhow::Result<Self> {
        Self::open_with_password(path, data, &[])
    }

    /// Open a document with an optional password.
    pub fn open_with_password(
        path: PathBuf,
        data: Arc<[u8]>,
        password: &[u8],
    ) -> anyhow::Result<Self> {
        let pdf = PdfDocument::open_with_password(data.clone(), password)
            .with_context(|| format!("opening PDF {}", path.display()))?;
        let page_count = pdf.page_count();
        let mut pages = Vec::with_capacity(page_count);

        for index in 0..page_count {
            let page = pdf.page(index)?;
            // effective_box() reflects what a viewer shows; apply rotation so
            // the reported dimensions match the rendered bitmap orientation.
            let visible = page.effective_box();
            let (user_width, user_height) = (
                (visible.x1 - visible.x0).abs(),
                (visible.y1 - visible.y0).abs(),
            );
            let rotate = page.rotate.rem_euclid(360);
            let (mut width, mut height) = (user_width, user_height);
            if rotate % 180 != 0 {
                std::mem::swap(&mut width, &mut height);
            }
            pages.push(PdfPageSummary {
                width,
                height,
                user_width,
                user_height,
                origin_x: visible.x0,
                origin_y: visible.y0,
                rotate,
            });
        }

        let (title, author) = pdf
            .info()
            .map_or((None, None), |info| (info.title, info.author));
        let outline = flatten_outline(&pdf.outline());
        let security = if pdf.is_encrypted() {
            PdfSecurity::Encrypted
        } else {
            PdfSecurity::Unencrypted
        };
        let page_labels = page_labels(&pdf, page_count);
        let form_widgets = collect_form_widgets(&pdf);

        Ok(Self {
            summary: PdfDocumentSummary {
                version: pdf.version(),
                page_count,
                pages,
                title,
                author,
                outline,
                security,
                page_labels,
                form_widgets,
            },
            data,
            password: password.to_vec(),
            path,
            pdf,
            colors: IccCache::new(),
            interpreted: InterpretedPages::default(),
        })
    }

    /// Replace this thread's copy with a document parsed from new bytes, keeping
    /// the password. Used after an edit is written back, and when another worker
    /// publishes a newer version.
    pub fn reload(&mut self, data: Arc<[u8]>) -> anyhow::Result<()> {
        *self = Self::open_with_password(self.path.clone(), data, &self.password)?;
        Ok(())
    }

    /// Summarize a candidate version of this document without adopting it. Used
    /// after an edit so the viewer receives the new summary together with the
    /// bytes, before it commits either.
    pub fn summarize(&self, data: Arc<[u8]>) -> anyhow::Result<PdfDocumentSummary> {
        let document = Self::open_with_password(self.path.clone(), data, &self.password)?;
        Ok(document.summary.clone())
    }

    /// The page's interpreted form: a display list plus the fonts and images it
    /// refers to. Interpretation includes stored annotation and form appearances,
    /// so a highlight another application wrote shows up in the rendered page.
    fn interpreted_page(&mut self, index: usize) -> anyhow::Result<Arc<InterpretedPage>> {
        if let Some(page) = self.interpreted.get(index) {
            return Ok(page);
        }
        let page = interpret_page_with_cache(
            &self.pdf,
            index,
            PageInterpretOptions::default(),
            &mut self.colors,
        )
        .with_context(|| format!("interpreting PDF page {index}"))?;
        let page = Arc::new(page);
        self.interpreted.insert(index, page.clone());
        Ok(page)
    }

    /// Render a single page to an RGBA bitmap at the given DPI. Only rasterizing
    /// happens here, so a zoom or display-scale change reuses the interpretation.
    pub fn render_page_preview(&mut self, index: usize, dpi: f32) -> anyhow::Result<PagePreview> {
        let interpreted = self.interpreted_page(index)?;
        let scale = dpi / 72.0;
        let mut renderer = zpdf::cpu::CpuRenderer::new()
            .with_fonts(&interpreted.fonts)
            .with_images(&interpreted.images);
        let rendered_page = renderer.render_display_list(&interpreted.display_list, scale)?;
        let blank_render_suspected = self.summary.security == PdfSecurity::Encrypted
            && interpreted.content_bytes > 0
            && rendered_page.data.chunks_exact(4).all(|pixel| {
                (pixel[3] == 0) || (pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255)
            });
        let image = render_image_from_rgba(
            rendered_page.width,
            rendered_page.height,
            rendered_page.data,
        )
        .context("building GPUI image from PDF page")?;

        Ok(PagePreview {
            image,
            scale,
            blank_render_suspected,
        })
    }

    /// Extract the page's text with geometry, used for search. The spans are in
    /// page space with the page rotation applied, matching what was rendered, so
    /// a hit rectangle maps onto the bitmap the user is looking at.
    pub fn page_spans(&mut self, index: usize) -> anyhow::Result<Vec<TextSpan>> {
        let page = self.pdf.page(index)?;
        let mut fonts = self.pdf.load_page_fonts(&page);
        let mut images = ImageCache::new();
        let content = self.pdf.page_content_bytes(&page)?;

        let mut spans: Vec<TextSpan> = Vec::new();
        ContentInterpreter::new(page.effective_box())
            .with_page_rotation(page.rotate)
            .with_fonts(&mut fonts)
            .with_document(self.pdf.file(), &page.resources)
            .with_images(&mut images)
            .with_text_sink(&mut spans)
            .interpret(&content);
        Ok(spans)
    }

    /// The page's text as one string, for copying. Line breaks follow the
    /// spans' baselines, which is what [`spans_to_text`] already computes.
    pub fn page_text(&mut self, index: usize) -> anyhow::Result<String> {
        Ok(spans_to_text(self.page_spans(index)?, 2.0))
    }

    /// The page's annotations with their object ids, so the view can offer them
    /// for deletion and editing. Read from the page's `/Annots` directly because
    /// zpdf's parsed [`zpdf::Annotation`] drops the id, and the id is what the
    /// writer needs.
    pub fn page_annotations(&mut self, index: usize) -> anyhow::Result<Vec<PageAnnotation>> {
        let page = self.pdf.page(index)?;
        let mut annotations = Vec::with_capacity(page.annots.len());
        for &id in &page.annots {
            let Ok(object) = self.pdf.file().resolve(id) else {
                continue;
            };
            let Ok(dict) = object.as_dict() else {
                continue;
            };
            let Some(rect) = dict_rect(dict, "Rect") else {
                continue;
            };
            let subtype = dict
                .get("Subtype")
                .and_then(|value| value.as_name().ok().map(str::to_owned))
                .unwrap_or_default();
            let contents = dict_text(dict, "Contents");
            let author = dict_text(dict, "T");
            let flags = dict
                .get("F")
                .and_then(|value| value.as_i64().ok())
                .unwrap_or(0);
            let (dest_page, uri) = self.resolve_annotation_target(dict);
            annotations.push(PageAnnotation {
                id,
                page: index,
                subtype,
                rect,
                contents,
                author,
                dest_page,
                uri,
                flags,
            });
        }
        Ok(annotations)
    }

    /// Resolve an annotation's link target: either an in-document destination
    /// page or an external URI. zpdf's destination resolver handles the `/Dest`
    /// forms (array, name, go-to action) so only the action dispatch is here.
    fn resolve_annotation_target(&self, dict: &zpdf::PdfDict) -> (Option<usize>, Option<String>) {
        if let Some(dest) = dict.get("Dest")
            && let Some(page) = self
                .pdf
                .resolve_destination(dest)
                .and_then(|dest| dest.page)
        {
            return (Some(page), None);
        }
        let Some(action) = dict.get("A").and_then(|value| self.deref(value)) else {
            return (None, None);
        };
        let Ok(action) = action.as_dict() else {
            return (None, None);
        };
        match action
            .get("S")
            .and_then(|value| value.as_name().ok())
            .unwrap_or_default()
        {
            "GoTo" => {
                let page = action
                    .get("D")
                    .and_then(|dest| self.pdf.resolve_destination(dest))
                    .and_then(|dest| dest.page);
                (page, None)
            }
            "URI" => (None, dict_text(action, "URI")),
            _ => (None, None),
        }
    }

    /// Follow one level of indirection, since actions and appearance
    /// dictionaries are usually references.
    fn deref(&self, object: &PdfObject) -> Option<PdfObject> {
        match object {
            PdfObject::Ref(id) => self.pdf.file().resolve(*id).ok(),
            other => Some(other.clone()),
        }
    }

    /// Document-level details the sidebar shows: metadata, signatures,
    /// attachments, output intents, and tagging/optional-content state.
    pub fn document_details(&mut self) -> anyhow::Result<PdfDocumentDetails> {
        let info = self.pdf.info().unwrap_or_default();
        let xmp = self.pdf.xmp_metadata().map(|xmp| XmpSummary {
            title: xmp.title,
            creators: xmp.creators,
            description: xmp.description,
            subjects: xmp.subjects,
            producer: xmp.producer,
            creator_tool: xmp.creator_tool,
            modify_date: xmp.modify_date,
        });
        let signatures = self
            .pdf
            .signatures()
            .into_iter()
            .map(|signature| SignatureInfo {
                field_name: signature.field_name,
                signing_time: signature.signing_time,
                reason: signature.reason,
                location: signature.location,
                coverage: if signature.coverage.covers_whole_document {
                    "whole document".to_owned()
                } else {
                    "partial".to_owned()
                },
                digest: signature.digest.as_str().to_owned(),
                crypto: signature.crypto.as_str().to_owned(),
            })
            .collect();
        let attachments = self
            .pdf
            .embedded_files()
            .into_iter()
            .map(|file| AttachmentInfo {
                name: file.name,
                description: file.description,
                mime_type: file.subtype,
                size: file.size,
            })
            .collect();
        let output_intents = self
            .pdf
            .output_intents()
            .into_iter()
            .map(|intent| match intent.output_condition {
                Some(condition) => format!("{} ({condition})", intent.subtype),
                None => intent.subtype,
            })
            .collect();

        Ok(PdfDocumentDetails {
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: info.keywords,
            creator: info.creator,
            producer: info.producer,
            creation_date: info.creation_date,
            mod_date: info.mod_date,
            xmp,
            signatures,
            attachments,
            output_intents,
            has_optional_content: self.pdf.oc_config().is_some(),
            is_tagged: self.pdf.is_tagged(),
        })
    }

    /// Apply edits as one incremental update and return the new file bytes.
    ///
    /// An incremental update appends to the original bytes, so the result is the
    /// whole new file: the caller writes it back and reopens from it. Form
    /// values share a single filler session so `/NeedAppearances` is flushed
    /// once rather than once per field.
    pub fn apply_edits(&mut self, edits: &[PdfEdit]) -> anyhow::Result<Vec<u8>> {
        if edits.is_empty() {
            anyhow::bail!("no edits requested");
        }
        let mut writer = if self.password.is_empty() {
            IncrementalWriter::new(self.data.to_vec())
        } else {
            IncrementalWriter::new_with_password(self.data.to_vec(), &self.password)
        }
        .context("opening the PDF for editing")?;

        let field_values: Vec<(&str, &str)> = edits
            .iter()
            .filter_map(|edit| match edit {
                PdfEdit::SetField { name, value } => Some((name.as_str(), value.as_str())),
                _ => None,
            })
            .collect();
        if !field_values.is_empty() {
            let mut filler = FormFiller::new(&mut writer)
                .context("the document has no interactive form to fill")?;
            for (name, value) in field_values {
                filler
                    .set(name, value)
                    .with_context(|| format!("setting form field {name:?}"))?;
            }
            filler.finish().context("flushing form appearance flags")?;
        }

        for edit in edits {
            match edit {
                PdfEdit::SetField { .. } => {}
                PdfEdit::Markup {
                    page,
                    kind,
                    source,
                    contents,
                } => {
                    let MarkupSource::Region(region) = source;
                    let rects = self.markup_rects(*page, *region)?;
                    let spec = AnnotationSpec::markup_from_rects(
                        *kind,
                        &rects,
                        markup_color(*kind),
                        contents.clone(),
                    );
                    writer
                        .add_annotation(*page, &spec)
                        .with_context(|| format!("adding a markup annotation to page {page}"))?;
                }
                PdfEdit::Annotation { page, spec } => {
                    writer
                        .add_annotation(*page, spec)
                        .with_context(|| format!("adding an annotation to page {page}"))?;
                }
                PdfEdit::DeleteAnnotation { page, annot } => {
                    writer
                        .delete_annotation(*page, *annot)
                        .context("deleting the annotation")?;
                }
                PdfEdit::RotatePage { page, degrees } => {
                    writer
                        .rotate_page(*page, *degrees)
                        .with_context(|| format!("rotating page {page}"))?;
                }
            }
        }

        let mut output = std::io::Cursor::new(Vec::new());
        writer.write(&mut output).context("writing the update")?;
        Ok(output.into_inner())
    }

    /// Intersect a dragged region with the page's text, returning one rect per
    /// text box the region touches. Returns the region itself when the page has
    /// no text there (scanned page, drawing, or an image), so a markup tool never
    /// silently does nothing.
    ///
    /// The spans carry display-space geometry with the page rotation baked in,
    /// so the mapping runs display -> default user space before the annotation is
    /// written.
    fn markup_rects(&mut self, page_index: usize, region: Rect) -> anyhow::Result<Vec<Rect>> {
        let spans = self.page_spans(page_index)?;
        let summary = self
            .summary
            .pages
            .get(page_index)
            .context("page is no longer in the document")?;
        let mut rects = Vec::new();
        for span in &spans {
            let font_size = span.size.max(1.0) as f64;
            // Baseline-relative box: the descender below, the ascender above.
            // zpdf's spans are per show-text operation, so this is the closest
            // box the geometry supports without per-glyph metrics.
            let box_left = span.x;
            let box_right = span.x + span.advance;
            let (box_left, box_right) = (box_left.min(box_right), box_left.max(box_right));
            let box_bottom = span.y - font_size * 0.25;
            let box_top = span.y + font_size * 0.75;
            let overlap = Rect {
                x0: box_left.max(region.x0),
                y0: box_bottom.max(region.y0),
                x1: box_right.min(region.x1),
                y1: box_top.min(region.y1),
            };
            if overlap.x1 <= overlap.x0 || overlap.y1 <= overlap.y0 {
                continue;
            }
            rects.push(summary.from_display_rect(overlap));
        }
        if rects.is_empty() {
            rects.push(summary.from_display_rect(region));
        }
        Ok(rects)
    }
}

/// Flatten zpdf's nested outline into the depth-tagged list the sidebar renders.
/// zpdf resolves named destinations and go-to actions already, so the only work
/// left is the traversal and the safety bounds.
fn flatten_outline(outline: &[ZpdfOutlineItem]) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    push_outline_items(outline, 0, &mut items);
    items
}

fn push_outline_items(outline: &[ZpdfOutlineItem], depth: usize, items: &mut Vec<OutlineItem>) {
    if depth > MAX_OUTLINE_DEPTH {
        return;
    }
    for item in outline {
        if items.len() >= MAX_OUTLINE_ITEMS {
            return;
        }
        items.push(OutlineItem {
            title: item.title.clone(),
            page_index: item.dest.as_ref().and_then(|destination| destination.page),
            depth,
        });
        push_outline_items(&item.children, depth + 1, items);
    }
}

impl PdfPageSummary {
    /// Map a rectangle from absolute default page user space into display space
    /// (what the rendered bitmap shows: rotation applied, origin at zero).
    pub(crate) fn to_display_rect(&self, rect: (f64, f64, f64, f64)) -> Rect {
        let (x0, y0) = self.to_display_point(rect.0 - self.origin_x, rect.1 - self.origin_y);
        let (x1, y1) = self.to_display_point(rect.2 - self.origin_x, rect.3 - self.origin_y);
        Rect {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    /// Inverse of [`Self::to_display_rect`]: display space back to absolute
    /// default page user space, which is what annotation geometry stores.
    pub(crate) fn from_display_rect(&self, rect: Rect) -> Rect {
        let (x0, y0) = self.from_display_point(rect.x0, rect.y0);
        let (x1, y1) = self.from_display_point(rect.x1, rect.y1);
        Rect {
            x0: x0.min(x1) + self.origin_x,
            y0: y0.min(y1) + self.origin_y,
            x1: x0.max(x1) + self.origin_x,
            y1: y0.max(y1) + self.origin_y,
        }
    }

    /// Origin-relative user space to display space for a single point.
    fn to_display_point(&self, x: f64, y: f64) -> (f64, f64) {
        match self.rotate {
            90 => (y, x),
            180 => (self.user_width - x, y),
            270 => (self.user_height - y, self.user_width - x),
            _ => (x, self.user_height - y),
        }
    }

    /// Display space back to origin-relative user space for a single point.
    fn from_display_point(&self, x: f64, y: f64) -> (f64, f64) {
        match self.rotate {
            90 => (y, x),
            180 => (self.user_width - x, y),
            270 => (self.user_width - y, self.user_height - x),
            _ => (x, self.user_height - y),
        }
    }

    /// Display space to absolute default page user space for a single point,
    /// which is the space annotation geometry stores.
    pub(crate) fn from_display_point_abs(&self, x: f64, y: f64) -> (f64, f64) {
        let (x, y) = self.from_display_point(x, y);
        (x + self.origin_x, y + self.origin_y)
    }
}

/// Display label for every page, from the document's page-label number tree.
/// Falls back to the 1-based page number so the caller never special-cases a
/// missing tree.
fn page_labels(pdf: &PdfDocument, page_count: usize) -> Vec<String> {
    let labels = pdf.page_labels();
    (0..page_count)
        .map(|index| {
            labels
                .as_ref()
                .and_then(|labels| labels.label(index))
                .unwrap_or_else(|| (index + 1).to_string())
        })
        .collect()
}

/// Field flag bit 17: push button, which holds no value a reader can set.
const FIELD_FLAG_PUSHBUTTON: i64 = 1 << 16;

/// Every interactive-form widget paired with the page it appears on. Widgets are
/// found through the pages' `/Annots`, because that is the only place the widget
/// rectangle lives.
fn collect_form_widgets(pdf: &PdfDocument) -> Vec<FormWidgetInfo> {
    let Some(form) = pdf.acro_form() else {
        return Vec::new();
    };
    let mut widgets = Vec::new();
    for page_index in 0..pdf.page_count() {
        let Ok(page) = pdf.page(page_index) else {
            continue;
        };
        for &annot_id in &page.annots {
            let Some(field) = form.field_for_widget(annot_id) else {
                continue;
            };
            let Ok(object) = pdf.file().resolve(annot_id) else {
                continue;
            };
            let Ok(dict) = object.as_dict() else {
                continue;
            };
            let Some(rect) = dict_rect(dict, "Rect") else {
                continue;
            };
            let kind = match field.kind {
                FieldKind::Text => FormWidgetKind::Text,
                FieldKind::Signature => FormWidgetKind::Signature,
                FieldKind::Choice => FormWidgetKind::Choice,
                FieldKind::Button if field.flags & FIELD_FLAG_RADIO != 0 => FormWidgetKind::Radio,
                FieldKind::Button if field.flags & FIELD_FLAG_PUSHBUTTON != 0 => {
                    FormWidgetKind::Other
                }
                FieldKind::Button => FormWidgetKind::Checkbox,
                FieldKind::Unknown => FormWidgetKind::Other,
            };
            widgets.push(FormWidgetInfo {
                page: page_index,
                name: field.name.clone(),
                kind,
                rect,
                value: field.display_value(),
                readonly: field.flags & FIELD_FLAG_READONLY != 0,
                multiline: field.is_multiline(),
                password: field.is_password(),
                options: field
                    .options
                    .iter()
                    .map(|(_, display)| display.clone())
                    .collect(),
                on_state: match kind {
                    FormWidgetKind::Checkbox | FormWidgetKind::Radio => widget_on_state(pdf, dict),
                    _ => None,
                },
            });
        }
    }
    widgets
}

/// The on-state name of a checkbox or radio widget: the `/AP /N` state key other
/// than `Off`. Storing that name is what checks the box.
fn widget_on_state(pdf: &PdfDocument, widget: &zpdf::PdfDict) -> Option<String> {
    let appearance = deref_object(pdf, widget.get("AP")?)?;
    let normal = deref_object(pdf, appearance.as_dict().ok()?.get("N")?)?;
    let states = normal.as_dict().ok()?;
    states
        .0
        .keys()
        .map(|name| name.as_str())
        .find(|state| *state != "Off")
        .map(str::to_owned)
}

/// Follow one level of indirection. Actions and appearance dictionaries are
/// usually indirect references.
fn deref_object(pdf: &PdfDocument, object: &PdfObject) -> Option<PdfObject> {
    match object {
        PdfObject::Ref(id) => pdf.file().resolve(*id).ok(),
        other => Some(other.clone()),
    }
}

/// A `/Rect`-style array of four numbers.
fn dict_rect(dict: &zpdf::PdfDict, key: &str) -> Option<(f64, f64, f64, f64)> {
    let array = dict.get(key)?.as_array().ok()?;
    let mut values = array.iter().filter_map(|value| value.as_f64().ok());
    Some((
        values.next()?,
        values.next()?,
        values.next()?,
        values.next()?,
    ))
}

/// A text-string entry, decoded per the PDF text-string rules.
fn dict_text(dict: &zpdf::PdfDict, key: &str) -> Option<String> {
    let PdfObject::String(value) = dict.get(key)? else {
        return None;
    };
    let text = decode_pdf_text(value.as_bytes());
    (!text.is_empty()).then_some(text)
}

/// Decode a PDF text string: UTF-16BE when it starts with the byte-order mark,
/// otherwise bytes that are already UTF-8/ASCII for every producer that matters.
fn decode_pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Default colour per markup subtype, matching the conventions viewers use so a
/// highlight reads as a highlight.
fn markup_color(kind: MarkupKind) -> (f64, f64, f64) {
    match kind {
        MarkupKind::Highlight => (1.0, 0.86, 0.16),
        MarkupKind::Underline => (0.12, 0.4, 0.9),
        MarkupKind::StrikeOut => (0.86, 0.16, 0.16),
        MarkupKind::Squiggly => (0.9, 0.5, 0.1),
    }
}

fn render_image_from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Arc<RenderImage>> {
    // GPUI's direct render image path expects BGRA byte order.
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    let buffer = RgbaImage::from_raw(width, height, rgba)?;
    let frame = Frame::new(buffer);
    Some(Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::MarkupSource;

    /// Assemble a synthetic PDF from numbered object bodies (object `i + 1`),
    /// computing a valid xref table and a trailer whose /Root is object 1.
    fn build_pdf(objects: &[&str]) -> Vec<u8> {
        let mut buffer = Vec::from(&b"%PDF-1.7\n"[..]);
        let mut offsets = Vec::with_capacity(objects.len());
        for (index, body) in objects.iter().enumerate() {
            offsets.push(buffer.len());
            buffer.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
        }
        let xref_offset = buffer.len();
        buffer.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in &offsets {
            buffer.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        buffer.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        buffer
    }

    /// A two-page document with an info dictionary and a single-level outline
    /// pointing at the second page.
    fn sample_pdf() -> Vec<u8> {
        build_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Title (Sample Document) /Author (Test Author) >>",
            "<< /Type /Outlines /First 8 0 R /Last 8 0 R /Count 1 >>",
            "<< /Title (Chapter Two) /Parent 7 0 R /Dest [4 0 R /Fit] >>",
        ])
    }

    fn named_destination_pdf() -> Vec<u8> {
        build_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R /Names << /Dests 9 0 R >> >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Title (Named Destinations) /Author (Test Author) >>",
            "<< /Type /Outlines /First 8 0 R /Last 8 0 R /Count 1 >>",
            "<< /Title (Go Named) /Parent 7 0 R /Dest (chapter-two) >>",
            "<< /Names [(chapter-two) [4 0 R /Fit]] >>",
        ])
    }

    /// Two nesting levels: a parent bookmark whose child points at page 1.
    fn nested_outline_pdf() -> Vec<u8> {
        build_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Title (Nested Outline) >>",
            "<< /Type /Outlines /First 8 0 R /Last 8 0 R /Count 2 >>",
            "<< /Title (Part One) /Parent 7 0 R /First 9 0 R /Last 9 0 R /Count 1 /Dest [3 0 R /Fit] >>",
            "<< /Title (Section A) /Parent 8 0 R /Dest [4 0 R /Fit] >>",
        ])
    }

    /// One page carrying real text, so the search pipeline has spans to match.
    fn text_pdf() -> Vec<u8> {
        let content = "BT /F1 24 Tf 20 250 Td (Hello PDF World) Tj ET";
        let stream = format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        );
        build_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
             /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            &stream,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            "<< /Title (Text Document) >>",
        ])
    }

    fn encrypted_flag_pdf() -> Vec<u8> {
        let mut builder = zpdf_writer::DocumentBuilder::new();
        builder.add_page(200.0, 300.0);
        let unencrypted = builder.build().expect("test PDF should build");
        let file = zpdf::PdfFile::parse(unencrypted).expect("test PDF should parse");
        zpdf_writer::rewrite_pdf(
            &file,
            &zpdf_writer::RewriteOptions {
                encrypt: Some(zpdf_writer::EncryptionConfig::aes256("", "owner")),
                ..Default::default()
            },
        )
        .expect("test PDF should encrypt")
    }

    #[test]
    fn extracts_page_spans_that_search_can_locate() {
        let mut document = LoadedPdfDocument::open("text.pdf".into(), text_pdf().into())
            .expect("text PDF should parse");
        let spans = document.page_spans(0).expect("page text should extract");
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        assert!(
            text.contains("Hello PDF World"),
            "extracted text was {text:?}"
        );

        let hits = zpdf::search_spans(&spans, "pdf world", false);
        assert_eq!(hits.len(), 1, "one case-insensitive match");
        let rect = hits[0].rects.first().expect("hit carries a page-space box");
        // The box must land inside the 200x300pt page, not outside it.
        assert!(
            rect.x0 >= 0.0 && rect.x1 <= 200.0,
            "horizontal box {rect:?}"
        );
        assert!(rect.y0 >= 0.0 && rect.y1 <= 300.0, "vertical box {rect:?}");
    }

    /// One page carrying an interactive text field, for the form round trip.
    fn form_pdf() -> Vec<u8> {
        build_pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
             /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
             /Annots [5 0 R] /Contents 4 0 R >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [10 10 110 30] \
             /V (before) /DA (/Helv 12 Tf 0 g) /P 3 0 R >>",
            "<< /Title (Form Document) >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ])
    }

    #[test]
    fn adds_a_markup_annotation_that_survives_a_reopen() {
        let mut document = LoadedPdfDocument::open("sample.pdf".into(), sample_pdf().into())
            .expect("sample PDF should parse");
        assert!(
            document
                .page_annotations(0)
                .expect("annotations readable")
                .is_empty()
        );

        let region = Rect::new(20.0, 200.0, 180.0, 260.0);
        let bytes = document
            .apply_edits(&[PdfEdit::Markup {
                page: 0,
                kind: MarkupKind::Highlight,
                source: MarkupSource::Region(region),
                contents: Some("note".to_owned()),
            }])
            .expect("highlight should be written");

        let mut reopened = LoadedPdfDocument::open("sample.pdf".into(), bytes.into())
            .expect("edited PDF should reopen");
        let annotations = reopened.page_annotations(0).expect("annotations readable");
        let highlight = annotations
            .iter()
            .find(|annotation| annotation.subtype == "Highlight")
            .expect("the highlight is in the edited file");
        assert_eq!(highlight.contents.as_deref(), Some("note"));
        assert!(highlight.rect.2 > highlight.rect.0);
        // The page has no text, so the markup falls back to the dragged region.
        assert!(
            (highlight.rect.0 - 20.0).abs() < 2.0,
            "{:?}",
            highlight.rect
        );
    }

    #[test]
    fn deleting_an_annotation_removes_it_from_the_file() {
        // Each edit is built from the bytes the document was opened with, so the
        // next edit has to start from a document reopened on the previous result
        // — which is exactly what the worker does after publishing an edit.
        let mut bytes = sample_pdf();
        for _ in 0..2 {
            let mut document = LoadedPdfDocument::open("sample.pdf".into(), bytes.into())
                .expect("sample PDF should parse");
            bytes = document
                .apply_edits(&[PdfEdit::Markup {
                    page: 0,
                    kind: MarkupKind::Underline,
                    source: MarkupSource::Region(Rect::new(20.0, 200.0, 180.0, 220.0)),
                    contents: None,
                }])
                .expect("markup should be written");
        }

        let mut reopened = LoadedPdfDocument::open("sample.pdf".into(), bytes.into())
            .expect("edited PDF should reopen");
        let annotations = reopened.page_annotations(0).expect("annotations readable");
        let underline = annotations
            .iter()
            .find(|annotation| annotation.subtype == "Underline")
            .expect("underline exists")
            .id;
        let bytes = reopened
            .apply_edits(&[PdfEdit::DeleteAnnotation {
                page: 0,
                annot: underline,
            }])
            .expect("deleting should be written");

        let mut reopened = LoadedPdfDocument::open("sample.pdf".into(), bytes.into())
            .expect("edited PDF should reopen");
        let annotations = reopened.page_annotations(0).expect("annotations readable");
        assert_eq!(
            annotations
                .iter()
                .filter(|annotation| annotation.subtype == "Underline")
                .count(),
            1,
            "exactly one underline survives"
        );
    }

    #[test]
    fn fills_a_form_field_that_survives_a_reopen() {
        let mut document = LoadedPdfDocument::open("form.pdf".into(), form_pdf().into())
            .expect("form PDF should parse");
        let widget = document
            .summary
            .form_widgets
            .first()
            .expect("the form has one widget");
        assert_eq!(widget.name, "name");
        assert_eq!(widget.kind, FormWidgetKind::Text);
        assert_eq!(widget.value.as_deref(), Some("before"));

        let bytes = document
            .apply_edits(&[PdfEdit::SetField {
                name: "name".to_owned(),
                value: "after".to_owned(),
            }])
            .expect("form value should be written");

        let document = LoadedPdfDocument::open("form.pdf".into(), bytes.into())
            .expect("edited PDF should reopen");
        let widget = document
            .summary
            .form_widgets
            .first()
            .expect("the form still has one widget");
        assert_eq!(widget.value.as_deref(), Some("after"));
    }

    #[test]
    fn rotating_a_page_swaps_its_reported_dimensions() {
        let mut document = LoadedPdfDocument::open("sample.pdf".into(), sample_pdf().into())
            .expect("sample PDF should parse");
        let before = document.summary.pages[0].clone();
        assert_eq!(before.rotate, 0);

        let bytes = document
            .apply_edits(&[PdfEdit::RotatePage {
                page: 0,
                degrees: 90,
            }])
            .expect("rotation should be written");

        let document = LoadedPdfDocument::open("sample.pdf".into(), bytes.into())
            .expect("edited PDF should reopen");
        let after = &document.summary.pages[0];
        assert_eq!(after.rotate, 90);
        assert!((after.width - before.height).abs() < 0.5);
        assert!((after.height - before.width).abs() < 0.5);
    }

    #[test]
    fn display_and_user_space_round_trip_on_every_rotation() {
        let rect = Rect::new(10.0, 20.0, 110.0, 70.0);
        for rotate in [0, 90, 180, 270] {
            let page = PdfPageSummary {
                width: if rotate % 180 == 0 { 200.0 } else { 300.0 },
                height: if rotate % 180 == 0 { 300.0 } else { 200.0 },
                user_width: 200.0,
                user_height: 300.0,
                origin_x: 0.0,
                origin_y: 0.0,
                rotate,
            };
            let display = page.to_display_rect((rect.x0, rect.y0, rect.x1, rect.y1));
            let round_tripped = page.from_display_rect(display);
            assert!((round_tripped.x0 - rect.x0).abs() < 1e-6, "rotate {rotate}");
            assert!((round_tripped.y0 - rect.y0).abs() < 1e-6, "rotate {rotate}");
            assert!((round_tripped.x1 - rect.x1).abs() < 1e-6, "rotate {rotate}");
            assert!((round_tripped.y1 - rect.y1).abs() < 1e-6, "rotate {rotate}");
        }
    }

    #[test]
    fn parses_metadata_pages_and_outline() {
        let document = LoadedPdfDocument::open("sample.pdf".into(), sample_pdf().into())
            .expect("sample PDF should parse");
        let summary = &document.summary;

        assert_eq!(summary.page_count, 2);
        assert_eq!(summary.pages.len(), 2);
        assert_eq!(summary.title.as_deref(), Some("Sample Document"));
        assert_eq!(summary.author.as_deref(), Some("Test Author"));

        let page = &summary.pages[0];
        assert!((page.width - 200.0).abs() < 0.5);
        assert!((page.height - 300.0).abs() < 0.5);

        assert_eq!(summary.outline.len(), 1);
        let item = &summary.outline[0];
        assert_eq!(item.title, "Chapter Two");
        assert_eq!(item.depth, 0);
        // The /Dest references object 4 (the second page, index 1).
        assert_eq!(item.page_index, Some(1));
        assert_eq!(summary.security, PdfSecurity::Unencrypted);
    }

    #[test]
    fn resolves_named_destinations_from_names_tree() {
        let document = LoadedPdfDocument::open("named.pdf".into(), named_destination_pdf().into())
            .expect("named destination PDF should parse");
        let item = &document.summary.outline[0];
        assert_eq!(item.title, "Go Named");
        assert_eq!(item.page_index, Some(1));
    }

    #[test]
    fn nests_child_bookmarks_one_level_deeper() {
        let document = LoadedPdfDocument::open("nested.pdf".into(), nested_outline_pdf().into())
            .expect("nested outline PDF should parse");
        let summary = &document.summary;

        assert_eq!(summary.outline.len(), 2);
        assert_eq!(summary.outline[0].title, "Part One");
        assert_eq!(summary.outline[0].depth, 0);
        assert_eq!(summary.outline[0].page_index, Some(0));
        assert_eq!(summary.outline[1].title, "Section A");
        assert_eq!(summary.outline[1].depth, 1);
        assert_eq!(summary.outline[1].page_index, Some(1));
    }

    #[test]
    fn detects_encrypted_documents_from_trailer() {
        let document = LoadedPdfDocument::open("encrypted.pdf".into(), encrypted_flag_pdf().into())
            .expect("encrypted flag PDF should still parse");
        assert_eq!(document.summary.security, PdfSecurity::Encrypted);
    }

    #[test]
    fn reuses_one_interpretation_across_render_scales() {
        let mut document = LoadedPdfDocument::open("sample.pdf".into(), sample_pdf().into())
            .expect("sample PDF should parse");

        let first = document
            .render_page_preview(1, 144.0)
            .expect("page should render");
        assert_eq!(
            document.interpreted.entries.len(),
            1,
            "one page interpreted"
        );

        let second = document
            .render_page_preview(1, 288.0)
            .expect("page should re-render at a new scale");
        assert_eq!(
            document.interpreted.entries.len(),
            1,
            "a new scale must not re-interpret the page",
        );
        assert!((first.scale - 2.0).abs() < f32::EPSILON);
        assert!((second.scale - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn keeps_the_interpreted_page_cache_bounded() {
        let mut pages = InterpretedPages::default();
        let mut document = LoadedPdfDocument::open("sample.pdf".into(), sample_pdf().into())
            .expect("sample PDF should parse");

        for index in 0..INTERPRETED_PAGE_LIMIT {
            let page = document.interpreted_page(0).expect("page should interpret");
            pages.insert(index, page);
        }
        assert_eq!(pages.entries.len(), INTERPRETED_PAGE_LIMIT);
        assert!(pages.get(0).is_some(), "recently used entries survive");

        let page = document.interpreted_page(0).expect("page should interpret");
        pages.insert(INTERPRETED_PAGE_LIMIT, page);
        assert_eq!(
            pages.entries.len(),
            INTERPRETED_PAGE_LIMIT,
            "the cache never grows past its bound",
        );
        assert!(
            pages.entries.contains_key(&INTERPRETED_PAGE_LIMIT),
            "the newest entry is admitted",
        );
    }
}
