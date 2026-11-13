//! Images read for notes at the size they show at (ADR 0020): ImageIO decodes a photo straight
//! to that size (a JPEG skips the pixels it does not need), so a 12-megapixel photo shown 400
//! points wide takes about 2 MB instead of 48, off the main thread. What ImageIO does not read
//! (SVG) goes through `NSImage`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier, MainThreadBound};
use objc2::MainThreadMarker;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGImage;

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

/// The size of the image in `file`, in points, upright (EXIF rotation applied), read from its
/// header without decoding it.
pub fn natural_size(file: &Path) -> Option<NSSize> {
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

/// The image in `file` decoded for showing at `size` points, on any thread (ImageIO is
/// thread-safe); `None` when ImageIO does not read it.
fn decode(file: &Path, size: NSSize) -> Option<CFRetained<CGImage>> {
    let source = source(file)?;
    let pixels = size.width.max(size.height) * SCALE;
    // SAFETY: ImageIO's key constants live for the whole process.
    let keys = unsafe { [ns(kCGImageSourceCreateThumbnailFromImageAlways), ns(kCGImageSourceCreateThumbnailWithTransform), ns(kCGImageSourceThumbnailMaxPixelSize)] };
    let values = [NSNumber::numberWithBool(true), NSNumber::numberWithBool(true), NSNumber::numberWithDouble(pixels.ceil())];
    let options = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged; the options are ImageIO's.
    unsafe { source.thumbnail_at_index(0, Some(&*(Retained::as_ptr(&options).cast::<CFDictionary>()))) }
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
    state: RefCell<State>,
    /// Where it was last drawn (text container coordinates, its top).
    pub drawn_at: Cell<Option<f64>>,
    /// Called on the main thread once it is decoded, to draw it.
    on_ready: Box<dyn Fn()>,
}

impl DeferredImage {
    pub fn new(file: PathBuf, size: NSSize, on_ready: impl Fn() + 'static) -> Self {
        Self { file, size, state: RefCell::new(State::Empty), drawn_at: Cell::new(None), on_ready: Box::new(on_ready) }
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
        let (file, size) = (self.file.clone(), self.size);
        DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(DispatchQoS::UserInitiated)).exec_async(move || {
            let decoded = decode(&file, size);
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
    }

    /// Whether it is decoded (probe mode).
    pub fn is_decoded(&self) -> bool {
        matches!(*self.state.borrow(), State::Ready(_))
    }
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
