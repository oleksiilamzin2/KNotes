//! Images read for notes at the size they show at (ADR 0020): ImageIO decodes a photo straight
//! to that size (a JPEG skips the pixels it does not need), so a 12-megapixel photo shown 400
//! points wide takes about 2 MB instead of 48. What ImageIO does not read (SVG) goes through
//! `NSImage`.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

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
    let properties: &NSDictionary<NSString, AnyObject> = unsafe { &*(objc2_core_foundation::CFRetained::as_ptr(&properties).as_ptr().cast()) };
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

/// The image in `file` decoded for showing at `size` points.
pub fn load(file: &Path, size: NSSize) -> Option<Retained<NSImage>> {
    let Some(source) = source(file) else {
        return NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&file.to_string_lossy()));
    };
    let pixels = size.width.max(size.height) * SCALE;
    // SAFETY: ImageIO's key constants live for the whole process.
    let keys = unsafe { [ns(kCGImageSourceCreateThumbnailFromImageAlways), ns(kCGImageSourceCreateThumbnailWithTransform), ns(kCGImageSourceThumbnailMaxPixelSize)] };
    let values = [NSNumber::numberWithBool(true), NSNumber::numberWithBool(true), NSNumber::numberWithDouble(pixels.ceil())];
    let options = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged; the options are ImageIO's.
    let image = unsafe { source.thumbnail_at_index(0, Some(&*(Retained::as_ptr(&options).cast::<CFDictionary>()))) }?;
    Some(NSImage::initWithCGImage_size(NSImage::alloc(), &image, size))
}

/// An image decoded the first time it is drawn: TextKit makes fragments for paragraphs out
/// of view too, and only those in view draw.
pub struct DeferredImage {
    file: PathBuf,
    /// The size it is decoded for, in points.
    pub size: NSSize,
    image: OnceCell<Option<Retained<NSImage>>>,
}

impl DeferredImage {
    pub fn new(file: PathBuf, size: NSSize) -> Self {
        Self { file, size, image: OnceCell::new() }
    }

    pub fn get(&self) -> Option<&NSImage> {
        self.image.get_or_init(|| load(&self.file, self.size)).as_deref()
    }

    /// Whether it has been decoded (probe mode).
    pub fn is_decoded(&self) -> bool {
        self.image.get().is_some()
    }
}

fn source(file: &Path) -> Option<objc2_core_foundation::CFRetained<CGImageSource>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    // SAFETY: NSURL and CFURL are toll-free bridged; no options.
    unsafe { CGImageSource::with_url(&*(Retained::as_ptr(&url).cast::<CFURL>()), None) }
}

/// A CFString key as an NSString (toll-free bridged).
fn ns(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged.
    unsafe { &*(key as *const CFString).cast::<NSString>() }
}
