//! Images read for notes at the size they show at (ADR 0020): ImageIO decodes a photo straight
//! to that size (a JPEG skips the pixels it does not need), so a 12-megapixel photo shown 400
//! points wide takes about 2 MB instead of 48, off the main thread. What ImageIO does not read
//! (SVG) goes through `NSImage`. A PDF's page is drawn by CoreGraphics at that size (ADR 0022).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use dispatch2::{DispatchQoS, DispatchQueue, DispatchQueueAttr, DispatchRetained, GlobalQueueIdentifier, MainThreadBound};
use objc2::MainThreadMarker;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGPDFBox, CGPDFDocument, CGPDFPage, kCGColorSpaceSRGB};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_app_kit::NSImage;
use objc2_core_foundation::{CFDictionary, CFString, CFURL};
use objc2_foundation::{NSDictionary, NSNumber, NSSize, NSString, NSURL};
use objc2_image_io::{
    CGImageSource, kCGImagePropertyDPIWidth, kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceThumbnailMaxPixelSize,
};

/// Pixels per point the images are decoded for (Retina screens).
pub const SCALE: f64 = 2.0;

/// Whether `file` is a PDF (by its extension).
fn is_pdf(file: &Path) -> bool {
    file.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Page `page` (from 1; the first when `None` or past the end) of the PDF in `file`.
fn pdf_page(file: &Path, page: Option<u32>) -> Option<CFRetained<CGPDFPage>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: NSURL and CFURL are toll-free bridged.
    let document = CGPDFDocument::with_url(Some(unsafe { &*(Retained::as_ptr(&url).cast::<CFURL>()) }))?;
    let count = CGPDFDocument::number_of_pages(Some(&document));
    let number = page.map_or(1, |p| p as usize);
    CGPDFDocument::page(Some(&document), if (1..=count).contains(&number) { number } else { 1 })
}

/// Each page's size in points (rotation applied) of the PDF in `file`; `None` when it does not
/// open.
pub fn pdf_page_sizes(file: &Path) -> Option<Vec<NSSize>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: NSURL and CFURL are toll-free bridged.
    let document = CGPDFDocument::with_url(Some(unsafe { &*(Retained::as_ptr(&url).cast::<CFURL>()) }))?;
    let count = CGPDFDocument::number_of_pages(Some(&document));
    Some((1..=count).filter_map(|n| CGPDFDocument::page(Some(&document), n)).map(|p| page_size(&p)).collect())
}

/// How many pages the PDF in `file` has.
pub fn pdf_pages(file: &Path) -> Option<usize> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: NSURL and CFURL are toll-free bridged.
    let document = CGPDFDocument::with_url(Some(unsafe { &*(Retained::as_ptr(&url).cast::<CFURL>()) }))?;
    Some(CGPDFDocument::number_of_pages(Some(&document)))
}

/// A PDF page's size in points, its rotation applied.
fn page_size(page: &CGPDFPage) -> NSSize {
    let box_rect = CGPDFPage::box_rect(Some(page), CGPDFBox::CropBox);
    if CGPDFPage::rotation_angle(Some(page)) % 180 == 0 {
        NSSize::new(box_rect.size.width, box_rect.size.height)
    } else {
        NSSize::new(box_rect.size.height, box_rect.size.width)
    }
}

/// The size of the image in `file` (or of page `page` of a PDF), in points, upright (EXIF
/// rotation applied), read from its header without decoding it.
pub fn natural_size(file: &Path, page: Option<u32>) -> Option<NSSize> {
    if is_pdf(file) {
        return pdf_page(file, page).map(|p| page_size(&p));
    }
    let Some(source) = source(file) else {
        let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&file.to_string_lossy()))?;
        return Some(image.size());
    };
    // SAFETY: no options; the result is a dictionary of image properties.
    let properties = unsafe { source.properties_at_index(0, None) }?;
    // SAFETY: CFDictionary and NSDictionary are toll-free bridged.
    let properties: &NSDictionary<NSString, AnyObject> = unsafe { &*(CFRetained::as_ptr(&properties).as_ptr().cast()) };
    let number = |key: &CFString| properties.objectForKey(ns(key)).and_then(|o| o.downcast::<NSNumber>().ok()).map(|n| n.doubleValue());
    // SAFETY: ImageIO's key constants live for the whole process.
    let (width, height, dpi, orientation) = unsafe {
        (number(kCGImagePropertyPixelWidth)?, number(kCGImagePropertyPixelHeight)?, number(kCGImagePropertyDPIWidth), number(kCGImagePropertyOrientation))
    };
    let points = 72.0 / dpi.filter(|d| *d > 0.0).unwrap_or(72.0);
    // Orientations 5 to 8 turn the image a quarter.
    let turned = orientation.is_some_and(|o| (5.0..=8.0).contains(&o));
    let (width, height) = if turned { (height, width) } else { (width, height) };
    Some(NSSize::new(width * points, height * points))
}

/// The image in `file` (page `page` of a PDF) decoded for showing at `size` points, on any
/// thread (ImageIO and CoreGraphics are thread-safe); `None` when ImageIO does not read it.
fn decode(file: &Path, size: NSSize, page: Option<u32>) -> Option<CFRetained<CGImage>> {
    if is_pdf(file) {
        return draw_pdf_page(file, size, page);
    }
    let source = source(file)?;
    let pixels = size.width.max(size.height) * SCALE;
    // SAFETY: ImageIO's key constants live for the whole process.
    let keys = unsafe { [ns(kCGImageSourceCreateThumbnailFromImageAlways), ns(kCGImageSourceCreateThumbnailWithTransform), ns(kCGImageSourceThumbnailMaxPixelSize)] };
    let values = [NSNumber::numberWithBool(true), NSNumber::numberWithBool(true), NSNumber::numberWithDouble(pixels.ceil())];
    let options = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged; the options are ImageIO's.
    unsafe { source.thumbnail_at_index(0, Some(&*(Retained::as_ptr(&options).cast::<CFDictionary>()))) }
}

/// Page `page` of the PDF in `file` drawn `size` points large on white. (An image over
/// pixels of our own was tried: CoreGraphics then kept a decoded copy of each, and memory grew
/// as the pages went by.)
fn draw_pdf_page(file: &Path, size: NSSize, page: Option<u32>) -> Option<CFRetained<CGImage>> {
    let page = pdf_page(file, page)?;
    let (width, height) = ((size.width * SCALE).round().max(1.0), (size.height * SCALE).round().max(1.0));
    // SAFETY: CoreGraphics's colour space name constant lives for the whole process.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: no buffer given: CoreGraphics allocates one; 8-bit RGBA, alpha premultiplied.
    let context = unsafe { CGBitmapContextCreate(std::ptr::null_mut(), width as usize, height as usize, 8, 0, Some(&space), CGImageAlphaInfo::PremultipliedLast.0) }?;
    let all = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width, height));
    CGContext::set_rgb_fill_color(Some(&context), 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(Some(&context), all);
    // The page's box onto a rect its own (rotated) size, then that scaled to the bitmap.
    let natural = page_size(&page);
    CGContext::scale_ctm(Some(&context), width / natural.width, height / natural.height);
    let own = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(natural.width, natural.height));
    CGContext::concat_ctm(Some(&context), CGPDFPage::drawing_transform(Some(&page), CGPDFBox::CropBox, own, 0, true));
    CGContext::draw_pdf_page(Some(&context), Some(&page));
    CGBitmapContextCreateImage(Some(&context))
}

enum State {
    Empty,
    Loading,
    Ready(Retained<NSImage>),
    Failed,
}

/// An image decoded in the background the first time it is drawn (TextKit makes fragments
/// for paragraphs out of view too, and only those in view draw), and let go again when it is
/// far out of view.
pub struct DeferredImage {
    file: PathBuf,
    /// The size it is decoded for, in points.
    pub size: NSSize,
    /// A PDF's page.
    page: Option<u32>,
    state: RefCell<State>,
    /// Where it was last drawn (text container coordinates, its top).
    pub drawn_at: Cell<Option<f64>>,
    /// Called on the main thread once it is decoded, to draw it.
    on_ready: Box<dyn Fn()>,
    /// Set when it is let go before it is decoded: the decoding not started yet is skipped.
    cancelled: RefCell<Arc<AtomicBool>>,
}

impl DeferredImage {
    pub fn new(file: PathBuf, size: NSSize, page: Option<u32>, on_ready: impl Fn() + 'static) -> Self {
        Self { file, size, page, state: RefCell::new(State::Empty), drawn_at: Cell::new(None), on_ready: Box::new(on_ready), cancelled: RefCell::new(Arc::new(AtomicBool::new(false))) }
    }

    /// The image once decoded; until then `None`, and decoding starts.
    pub fn get(self: &Rc<Self>) -> Option<Retained<NSImage>> {
        if let State::Ready(image) = &*self.state.borrow() {
            return Some(image.clone());
        }
        self.start();
        None
    }

    fn start(self: &Rc<Self>) {
        if !matches!(*self.state.borrow(), State::Empty) {
            return;
        }
        *self.state.borrow_mut() = State::Loading;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let this = MainThreadBound::new(Rc::downgrade(self), mtm);
        let (file, size, page) = (self.file.clone(), self.size, self.page);
        let cancelled = Arc::new(AtomicBool::new(false));
        *self.cancelled.borrow_mut() = Arc::clone(&cancelled);
        // A PDF's pages one after another (scrolling past many would draw them all at once);
        // images side by side.
        let queue = if is_pdf(&file) { pdf_queue() } else { DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(DispatchQoS::UserInitiated)) };
        queue.exec_async(move || {
            // Let go before its turn: not drawn at all.
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            // Its own pool: the drawing's temporary objects go with it, not when the queue's
            // thread gets round to draining its pool.
            let decoded = objc2::rc::autoreleasepool(|_| decode(&file, size, page));
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                let Some(this) = this.get(mtm).upgrade() else { return };
                // Let go while it was decoding: far out of view by now.
                if !matches!(*this.state.borrow(), State::Loading) {
                    return;
                }
                let image = match decoded {
                    Some(image) => Some(NSImage::initWithCGImage_size(NSImage::alloc(), &image, size)),
                    // What ImageIO does not read (SVG).
                    None => NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&file.to_string_lossy())),
                };
                *this.state.borrow_mut() = image.map_or(State::Failed, State::Ready);
                (this.on_ready)();
            });
        });
    }

    /// Lets the decoded image go (or drops one being decoded); drawn again, it is decoded again.
    pub fn let_go(&self) {
        let mut state = self.state.borrow_mut();
        if matches!(*state, State::Ready(_) | State::Loading) {
            *state = State::Empty;
        }
        self.cancelled.borrow().store(true, Ordering::Relaxed);
    }

    /// Whether it is decoded (probe mode).
    pub fn is_decoded(&self) -> bool {
        matches!(*self.state.borrow(), State::Ready(_))
    }
}

/// The queue PDF pages are drawn on, one at a time.
fn pdf_queue() -> DispatchRetained<DispatchQueue> {
    static QUEUE: OnceLock<DispatchRetained<DispatchQueue>> = OnceLock::new();
    QUEUE.get_or_init(|| DispatchQueue::new("knotes.pdf-pages", DispatchQueueAttr::SERIAL)).clone()
}

fn source(file: &Path) -> Option<CFRetained<CGImageSource>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: NSURL and CFURL are toll-free bridged; no options.
    unsafe { CGImageSource::with_url(&*(Retained::as_ptr(&url).cast::<CFURL>()), None) }
}

/// A CFString key as an NSString (toll-free bridged).
fn ns(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged.
    unsafe { &*(key as *const CFString).cast::<NSString>() }
}
