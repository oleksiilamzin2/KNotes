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
use objc2_pdf_kit::{PDFActionGoTo, PDFDisplayBox, PDFDocument, PDFOutline, kPDFDestinationUnspecifiedValue};

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

/// Where `query` is in the PDF at `file`, ignoring case: each page (index) and the
/// characters there (indices into the page's text, as `PageText` counts them), in order.
/// `None` when the PDF does not open or `cancelled` is set while it searches (a page at a
/// time, its text let go after it).
pub fn find_in_pdf(file: &Path, query: &str, cancelled: &std::sync::atomic::AtomicBool) -> Option<Vec<(usize, std::ops::Range<usize>)>> {
    let needle: Vec<char> = query.chars().map(lower).collect();
    let document = open(file)?;
    let mut found = Vec::new();
    if needle.is_empty() {
        return Some(found);
    }
    // SAFETY: a document of its own, read on this thread only.
    for index in 0..unsafe { document.pageCount() } {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        autoreleasepool(|_| {
            // SAFETY: as above; the page is asked for within the count.
            let Some(text) = (unsafe { document.pageAtIndex(index).and_then(|p| p.string()) }) else { return };
            let chars: Vec<char> = text.to_string().chars().map(lower).collect();
            let mut at = 0;
            while at + needle.len() <= chars.len() {
                if chars[at..at + needle.len()] == needle[..] {
                    found.push((index, at..at + needle.len()));
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
        });
    }
    Some(found)
}

/// A character in lower case, one for one (indices stay the text's).
fn lower(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// An entry of a PDF's outline (its bookmarks).
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    /// 1 for the top level.
    pub level: u8,
    pub title: String,
    /// The page it goes to (index).
    pub page: usize,
    /// How far down the page as shown (a fraction; the top when the PDF says no height).
    pub y: f64,
}

/// The outline the PDF at `file` carries, in order, nested entries after their parent; empty
/// when it carries none, `None` when it does not open. Entries going nowhere are left out.
pub fn pdf_outline(file: &Path) -> Option<Vec<Bookmark>> {
    autoreleasepool(|_| {
        let document = open(file)?;
        let mut out = Vec::new();
        // SAFETY: the document's own outline, read on this thread only.
        if let Some(root) = unsafe { document.outlineRoot() } {
            unsafe { collect_bookmarks(&document, &root, 1, &mut out) };
        }
        Some(out)
    })
}

/// The children of `outline` at `level`, and theirs after each.
///
/// # Safety
/// `outline` belongs to `document`, both used on this thread only.
unsafe fn collect_bookmarks(document: &PDFDocument, outline: &PDFOutline, level: u8, out: &mut Vec<Bookmark>) {
    for i in 0..unsafe { outline.numberOfChildren() } {
        let Some(child) = (unsafe { outline.childAtIndex(i) }) else { continue };
        // A GoTo action is the other way an entry says where it goes.
        let destination = unsafe { child.destination() }
            .or_else(|| unsafe { child.action() }.and_then(|a| a.downcast::<PDFActionGoTo>().ok()).map(|a| unsafe { a.destination() }));
        if let Some((destination, page)) = destination.and_then(|d| Some((d.clone(), unsafe { d.page() }?))) {
            let point = unsafe { destination.point() };
            let crop = bounds(unsafe { page.boundsForBox(PDFDisplayBox::CropBox) });
            // A height the PDF leaves unsaid is the page's top.
            let y = if point.y >= unsafe { kPDFDestinationUnspecifiedValue } { crop.y + crop.height } else { point.y };
            let at = Bounds { x: crop.x, y, width: 0.0, height: 0.0 }.shown(crop, unsafe { page.rotation() } as i64);
            out.push(Bookmark {
                level,
                title: unsafe { child.label() }.map(|l| l.to_string()).unwrap_or_default(),
                page: unsafe { document.indexForPage(&page) },
                y: at.y.clamp(0.0, 1.0),
            });
        }
        unsafe { collect_bookmarks(document, &child, level.saturating_add(1), out) };
    }
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
