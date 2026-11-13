//! The text of PDF files, for the index and the search of the notes, and a page's with where
//! its characters are, for selecting it in the viewer (ADR 0022), through PDFKit: on any
//! thread, each call with its own document.

mod page_text;

pub use page_text::{Bounds, PageText};

use std::path::{Path, PathBuf};

use knotes_core::{DocumentText, FileKind};

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSRange, NSRect, NSString, NSURL};
use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};

/// The text of the PDF at `file`, its pages' in order; `None` when it does not open (not a PDF,
/// or locked).
pub fn pdf_text(file: &Path) -> Option<String> {
    let document = open(file)?;
    // SAFETY: a document of its own.
    Some(unsafe { document.string() }.map(|s| s.to_string()).unwrap_or_default())
}

/// The text of page `index` of the PDF at `file`, with where its characters are on the page
/// as shown; `None` when the PDF does not open or has no such page. The document goes when
/// this returns: nothing of PDFKit's stays.
pub fn page_text(file: &Path, index: usize) -> Option<PageText> {
    autoreleasepool(|_| {
        let document = open(file)?;
        // SAFETY: a document of its own; the page is asked for within its count.
        let page = unsafe { (index < document.pageCount()).then(|| document.pageAtIndex(index)).flatten() }?;
        // SAFETY: as above.
        let (crop, rotation, text) = unsafe { (bounds(page.boundsForBox(PDFDisplayBox::CropBox)), page.rotation() as i64, page.string()) };
        let text = text.map(|s| s.to_string()).unwrap_or_default();
        let mut at = 0;
        let glyphs: Vec<(char, Bounds)> = text
            .chars()
            .map(|ch| {
                // A selection's indices are the text's (UTF-16), as `characterBoundsAtIndex`'s
                // are not: they skip the line breaks PDFKit puts in the text.
                let range = NSRange::new(at, ch.len_utf16());
                at += range.length;
                // SAFETY: a range within the page's text, on the page's own document.
                let placed = autoreleasepool(|_| unsafe { page.selectionForRange(range).map(|s| bounds(s.boundsForPage(&page))) });
                (ch, placed.filter(|b| b.width > 0.0 && b.height > 0.0).map_or_else(Bounds::default, |b| b.shown(crop, rotation)))
            })
            .collect();
        Some(PageText::new(glyphs))
    })
}

/// The PDF at `file`, a document of its own.
fn open(file: &Path) -> Option<Retained<PDFDocument>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: a document read from a file URL.
    unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
}

fn bounds(rect: NSRect) -> Bounds {
    Bounds { x: rect.origin.x, y: rect.origin.y, width: rect.size.width, height: rect.size.height }
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
