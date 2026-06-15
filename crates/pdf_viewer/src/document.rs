use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Context as _;
use gpui::RenderImage;
use image::{Frame, RgbaImage};
use smallvec::SmallVec;
use zpdf::{
    ContentInterpreter, IccCache, ImageCache, ObjectId, PdfDocument, PdfObject, RenderBackend,
    TextSpan, spans_to_text,
};

/// A parsed PDF document plus precomputed summary information. This type owns
/// the underlying [`zpdf::PdfDocument`], which is neither `Send` nor `Sync`, so
/// it must live on a single dedicated thread (see `worker.rs`).
pub struct LoadedPdfDocument {
    pub summary: PdfDocumentSummary,
    pdf: PdfDocument,
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
}

#[derive(Debug, Clone)]
pub struct PdfPageSummary {
    /// Page width in PDF points after accounting for rotation.
    pub width: f64,
    /// Page height in PDF points after accounting for rotation.
    pub height: f64,
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

/// A rendered page bitmap ready to be displayed by GPUI. Cheaply clonable.
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

impl LoadedPdfDocument {
    /// Open a document from shared bytes. The `Arc<[u8]>` is handed straight to
    /// zpdf, so reopening the same buffer on several worker threads shares the
    /// underlying bytes and only duplicates the per-thread parse caches.
    pub fn open(path: PathBuf, data: Arc<[u8]>) -> anyhow::Result<Self> {
        let pdf =
            PdfDocument::open(data).with_context(|| format!("opening PDF {}", path.display()))?;
        let page_count = pdf.page_count();
        let mut pages = Vec::with_capacity(page_count);
        let mut page_id_to_index = HashMap::with_capacity(page_count);

        for index in 0..page_count {
            let page = pdf.page(index)?;
            // effective_box() reflects what a viewer shows; apply rotation so
            // the reported dimensions match the rendered bitmap orientation.
            let visible = page.effective_box();
            let (mut width, mut height) = (
                (visible.x1 - visible.x0).abs(),
                (visible.y1 - visible.y0).abs(),
            );
            if page.rotate % 180 != 0 {
                std::mem::swap(&mut width, &mut height);
            }
            page_id_to_index.insert(page.id, index);
            pages.push(PdfPageSummary { width, height });
        }

        let (title, author) = document_info(&pdf);
        let named_destinations = parse_named_destinations(&pdf, &page_id_to_index);
        let outline = parse_outline(&pdf, &page_id_to_index, &named_destinations);
        let security = if pdf.file().trailer.get("Encrypt").is_some() {
            PdfSecurity::Encrypted
        } else {
            PdfSecurity::Unencrypted
        };

        Ok(Self {
            summary: PdfDocumentSummary {
                version: pdf.version(),
                page_count,
                pages,
                title,
                author,
                outline,
                security,
            },
            pdf,
        })
    }

    /// Render a single page to an RGBA bitmap at the given DPI.
    pub fn render_page_preview(&self, index: usize, dpi: f32) -> anyhow::Result<PagePreview> {
        let page = self.pdf.page(index)?;
        let mut fonts = self.pdf.load_page_fonts(&page);
        let mut images = ImageCache::new();
        let mut colors = IccCache::new();
        let content = self.pdf.page_content_bytes(&page)?;

        let display_list = ContentInterpreter::new(page.effective_box())
            .with_page_rotation(page.rotate)
            .with_fonts(&mut fonts)
            .with_document(self.pdf.file(), &page.resources)
            .with_images(&mut images)
            .with_colors(&mut colors)
            .interpret(&content);

        let scale = dpi / 72.0;
        let mut renderer = zpdf::cpu::CpuRenderer::new()
            .with_fonts(&fonts)
            .with_images(&images);
        let rendered_page = renderer.render_display_list(&display_list, scale)?;
        let blank_render_suspected = self.summary.security == PdfSecurity::Encrypted
            && !content.is_empty()
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

    /// Extract the plain text of a page, used for search.
    pub fn page_text(&self, index: usize) -> anyhow::Result<String> {
        let page = self.pdf.page(index)?;
        let mut fonts = self.pdf.load_page_fonts(&page);
        let mut images = ImageCache::new();
        let content = self.pdf.page_content_bytes(&page)?;

        let mut spans: Vec<TextSpan> = Vec::new();
        {
            let interpreter = ContentInterpreter::new(page.effective_box())
                .with_fonts(&mut fonts)
                .with_document(self.pdf.file(), &page.resources)
                .with_images(&mut images)
                .with_text_sink(&mut spans);
            interpreter.interpret(&content);
        }
        Ok(spans_to_text(spans, 2.0))
    }
}

/// Pull `/Title` and `/Author` out of the document information dictionary.
fn document_info(pdf: &PdfDocument) -> (Option<String>, Option<String>) {
    let file = pdf.file();
    let Ok(info_ref) = file.trailer.get_ref("Info") else {
        return (None, None);
    };
    let Ok(info) = file.resolve(info_ref) else {
        return (None, None);
    };
    let Ok(dict) = info.as_dict() else {
        return (None, None);
    };
    let read = |key: &str| {
        dict.get(key)
            .and_then(|value| value.as_str().ok())
            .map(|string| decode_pdf_text(string.as_bytes()))
            .filter(|string| !string.is_empty())
    };
    (read("Title"), read("Author"))
}

/// Walk the `/Outlines` tree into a flat, depth-tagged list. Cycles and
/// excessive depth are guarded so malformed documents cannot hang the parser.
fn parse_outline(
    pdf: &PdfDocument,
    page_id_to_index: &HashMap<ObjectId, usize>,
    named_destinations: &HashMap<String, usize>,
) -> Vec<OutlineItem> {
    const MAX_OUTLINE_DEPTH: usize = 32;
    const MAX_OUTLINE_ITEMS: usize = 4096;

    let file = pdf.file();
    let mut items = Vec::new();
    let mut visited = HashSet::new();

    let Ok(root_ref) = file.trailer.get_ref("Root") else {
        return items;
    };
    let Ok(root) = file.resolve(root_ref) else {
        return items;
    };
    let Ok(root_dict) = root.as_dict() else {
        return items;
    };
    let Ok(outlines_ref) = root_dict.get_ref("Outlines") else {
        return items;
    };
    let Ok(outlines) = file.resolve(outlines_ref) else {
        return items;
    };
    let Ok(outlines_dict) = outlines.as_dict() else {
        return items;
    };

    let mut stack: Vec<(ObjectId, usize)> = Vec::new();
    if let Ok(first) = outlines_dict.get_ref("First") {
        stack.push((first, 0));
    }

    // Depth-first traversal that preserves sibling order by pushing the next
    // sibling before descending into children.
    while let Some((node_id, depth)) = stack.pop() {
        if depth > MAX_OUTLINE_DEPTH || items.len() >= MAX_OUTLINE_ITEMS {
            continue;
        }
        if !visited.insert(node_id) {
            continue;
        }
        let Ok(node) = file.resolve(node_id) else {
            continue;
        };
        let Ok(node_dict) = node.as_dict() else {
            continue;
        };

        let title = node_dict
            .get("Title")
            .and_then(|value| value.as_str().ok())
            .map(|string| decode_pdf_text(string.as_bytes()))
            .unwrap_or_default();
        let page_index =
            resolve_destination_page(file, node_dict, page_id_to_index, named_destinations);

        if let Ok(next) = node_dict.get_ref("Next") {
            stack.push((next, depth));
        }
        if let Ok(child) = node_dict.get_ref("First") {
            stack.push((child, depth + 1));
        }

        items.push(OutlineItem {
            title,
            page_index,
            depth,
        });
    }

    items
}

/// Resolve an outline item's `/Dest` or `/A` (GoTo) action to a 0-based page
/// index by matching the destination page object id.
fn resolve_destination_page(
    file: &zpdf::PdfFile,
    node_dict: &zpdf::PdfDict,
    page_id_to_index: &HashMap<ObjectId, usize>,
    named_destinations: &HashMap<String, usize>,
) -> Option<usize> {
    let dest = node_dict.get("Dest").cloned().or_else(|| {
        let action = node_dict.get_ref("A").ok()?;
        let action = file.resolve(action).ok()?;
        let action_dict = action.as_dict().ok()?;
        action_dict.get("D").cloned()
    })?;

    resolve_destination_object(file, &dest, page_id_to_index, named_destinations)
}

fn resolve_destination_object(
    file: &zpdf::PdfFile,
    dest: &PdfObject,
    page_id_to_index: &HashMap<ObjectId, usize>,
    named_destinations: &HashMap<String, usize>,
) -> Option<usize> {
    match dest {
        PdfObject::Ref(id) => {
            let resolved = file.resolve(*id).ok()?;
            resolve_destination_object(file, &resolved, page_id_to_index, named_destinations)
        }
        PdfObject::Array(array) => match array.first()? {
            PdfObject::Ref(id) => page_id_to_index.get(id).copied(),
            _ => None,
        },
        PdfObject::Dict(dict) => {
            let dest = dict.get("D")?;
            resolve_destination_object(file, dest, page_id_to_index, named_destinations)
        }
        PdfObject::String(string) => named_destinations
            .get(&decode_pdf_text(string.as_bytes()))
            .copied(),
        PdfObject::Name(name) => named_destinations.get(name.as_str()).copied(),
        _ => None,
    }
}

fn parse_named_destinations(
    pdf: &PdfDocument,
    page_id_to_index: &HashMap<ObjectId, usize>,
) -> HashMap<String, usize> {
    const MAX_NAME_TREE_NODES: usize = 2048;

    let file = pdf.file();
    let Ok(root_ref) = file.trailer.get_ref("Root") else {
        return HashMap::default();
    };
    let Ok(root) = file.resolve(root_ref) else {
        return HashMap::default();
    };
    let Ok(root_dict) = root.as_dict() else {
        return HashMap::default();
    };

    let mut destinations = HashMap::default();

    if let Some(dests_dict) = root_dict
        .get("Dests")
        .and_then(|value| resolve_object(file, value))
        .and_then(|value| value.as_dict().ok().cloned())
    {
        for (name, value) in &dests_dict.0 {
            if let Some(page_index) =
                resolve_destination_object(file, value, page_id_to_index, &destinations)
            {
                destinations.insert(name.as_str().to_owned(), page_index);
            }
        }
    }

    let Some(names_root) = root_dict
        .get("Names")
        .and_then(|value| resolve_object(file, value))
        .and_then(|value| value.as_dict().ok().cloned())
    else {
        return destinations;
    };
    let Some(dests_root) = names_root.get("Dests") else {
        return destinations;
    };

    let mut visited: HashSet<ObjectId> = HashSet::default();
    let mut stack = vec![dests_root.clone()];

    while let Some(node) = stack.pop() {
        if visited.len() >= MAX_NAME_TREE_NODES {
            break;
        }

        let resolved = match &node {
            PdfObject::Ref(id) => {
                if !visited.insert(*id) {
                    continue;
                }
                file.resolve(*id).ok()
            }
            other => Some(other.clone()),
        };
        let Some(resolved) = resolved else {
            continue;
        };
        let Ok(node_dict) = resolved.as_dict() else {
            continue;
        };

        if let Ok(names) = node_dict.get_array("Names") {
            for pair in names.chunks(2) {
                let [name, dest] = pair else { continue };
                let Some(name) = destination_name(name) else {
                    continue;
                };
                if let Some(page_index) =
                    resolve_destination_object(file, dest, page_id_to_index, &destinations)
                {
                    destinations.insert(name, page_index);
                }
            }
        }

        if let Ok(kids) = node_dict.get_array("Kids") {
            stack.extend(kids.iter().cloned());
        }
    }

    destinations
}

fn destination_name(object: &PdfObject) -> Option<String> {
    match object {
        PdfObject::String(string) => Some(decode_pdf_text(string.as_bytes())),
        PdfObject::Name(name) => Some(name.as_str().to_owned()),
        _ => None,
    }
}

fn resolve_object(file: &zpdf::PdfFile, object: &PdfObject) -> Option<PdfObject> {
    match object {
        PdfObject::Ref(id) => file.resolve(*id).ok(),
        other => Some(other.clone()),
    }
}

/// Decode a PDF text string, honouring the UTF-16BE byte-order-mark convention
/// used for non-ASCII titles. Falls back to lossy UTF-8 otherwise.
fn decode_pdf_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
    .trim()
    .to_owned()
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

    fn encrypted_flag_pdf() -> Vec<u8> {
        let mut buffer = Vec::from(&b"%PDF-1.7\n"[..]);
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
            "<< /Filter /Standard /V 1 /R 2 /O <0000000000000000000000000000000000000000000000000000000000000000> /U <0000000000000000000000000000000000000000000000000000000000000000> /P -4 >>",
        ];
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
                "trailer\n<< /Size {} /Root 1 0 R /Encrypt 4 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        buffer
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
    fn detects_encrypted_documents_from_trailer() {
        let document = LoadedPdfDocument::open("encrypted.pdf".into(), encrypted_flag_pdf().into())
            .expect("encrypted flag PDF should still parse");
        assert_eq!(document.summary.security, PdfSecurity::Encrypted);
    }

    #[test]
    fn decodes_utf16_be_titles() {
        // FEFF BOM followed by "Hi" in UTF-16BE.
        let bytes = [0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69];
        assert_eq!(decode_pdf_text(&bytes), "Hi");
        assert_eq!(decode_pdf_text(b"Plain"), "Plain");
    }
}
