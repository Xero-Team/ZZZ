//! Edits requested by the viewer and applied to the PDF on disk.
//!
//! Every edit is a zpdf incremental update: the original bytes are copied
//! verbatim and only new or modified objects are appended, so a failed write
//! leaves the document untouched. Encoding the request as data (rather than
//! running the writer on the UI thread) keeps the `!Send` [`zpdf::PdfDocument`]
//! and everything it borrows on the worker that owns it.

use zpdf::{AnnotationSpec, MarkupKind, ObjectId, Rect};

/// A single user-visible change to the document.
#[derive(Clone, Debug)]
pub(crate) enum PdfEdit {
    /// A text-markup annotation over text. `source` says where the geometry
    /// comes from; a region is resolved against the page's text spans on the
    /// worker, which is the only place those spans live.
    Markup {
        page: usize,
        kind: MarkupKind,
        source: MarkupSource,
        contents: Option<String>,
    },
    /// Any other annotation zpdf can synthesize an appearance for: a sticky
    /// note, free text, a rectangle, an ellipse, or a line.
    Annotation { page: usize, spec: AnnotationSpec },
    /// Remove one annotation from its page.
    DeleteAnnotation { page: usize, annot: ObjectId },
    /// Set an interactive-form field's value. `value` is the field's stored
    /// value: a choice field's export value, or a button's on-state name.
    SetField { name: String, value: String },
    /// Set a page's `/Rotate` to an absolute clockwise angle.
    RotatePage { page: usize, degrees: i32 },
}

/// Where a text-markup annotation's quads come from: a region the user dragged,
/// which the worker intersects with the page's text spans so the markup hugs the
/// text instead of the drag box.
#[derive(Clone, Debug)]
pub(crate) enum MarkupSource {
    Region(Rect),
}

/// The result of applying edits: the new file bytes plus the summary of the
/// document as reopened from them, so the viewer can adopt both in one step.
#[derive(Clone)]
pub(crate) struct EditOutcome {
    pub bytes: std::sync::Arc<[u8]>,
    pub summary: std::sync::Arc<crate::document::PdfDocumentSummary>,
}
