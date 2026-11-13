//! The text of PDF files, for the index and the search of the notes, and a page's with where
//! its characters are, for selecting it in the viewer (ADR 0022), through PDFKit: on any
//! thread, each call with its own document.

mod page_text;

pub use page_text::{Bounds, PageText};

use std::path::{Path, PathBuf};

use knotes_core::{DocumentText, FileKind};

use objc2::AnyThread;
use objc2_foundation::{NSString, NSURL};
use objc2_pdf_kit::PDFDocument;

/// The text of the PDF at `file`, its pages' in order; `None` when it does not open (not a PDF,
/// or locked).
pub fn pdf_text(file: &Path) -> Option<String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: a document of its own, read from a file URL.
    let document = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }?;
    // SAFETY: as above.
    Some(unsafe { document.string() }.map(|s| s.to_string()).unwrap_or_default())
}

/// The text of a vault's PDFs, for the index.
pub struct PdfTexts {
    root: PathBuf,
}

impl PdfTexts {
    /// For the vault at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl DocumentText for PdfTexts {
    fn document_text(&self, path: &str) -> Option<String> {
        (FileKind::from_path(path) == FileKind::Pdf).then(|| pdf_text(&self.root.join(path))).flatten()
    }
}
