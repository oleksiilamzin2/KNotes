//! The text of PDF files, for the index and the search of the notes (ADR 0022), through
//! PDFKit: on any thread, each call with its own document.

use std::path::Path;

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
