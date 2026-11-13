//! A PDF opened in place of the note (ADR 0022): its pages one under another, fitted to the
//! column's width, in a scroll view that zooms. Only the page shown and two above and two
//! below it are kept drawn, with any others in view (owner's choice); the rest are let go, and
//! everything goes with the viewer. Pages are drawn by CoreGraphics in the background, as
//! images are (ADR 0020). PDFKit's view kept every page it drew while it lived.
//!
//! The document view only gives the scroll view the pages' height; a canvas the size of the
//! visible part follows it and draws what shows. Drawn into the document view itself, the
//! pages' whole height was layer tiles Core Animation let go of only in batches, and memory
//! piled up as the pages went by.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAffineTransformNSAppKitAdditions, NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSCompositingOperation, NSEvent, NSResponder, NSScrollView, NSScrollViewDidEndLiveMagnifyNotification, NSView,
    NSViewBoundsDidChangeNotification,
};
use objc2_foundation::{NSAffineTransform, NSNotification, NSNotificationCenter, NSObject, NSPoint, NSRect, NSSize, NSZeroRect};

use crate::image_loader::{self, DeferredImage};

/// Space around and between pages.
const MARGIN: f64 = 16.0;
const GAP: f64 = 12.0;
/// Pages kept drawn above and below the one shown.
const KEPT_AROUND: usize = 2;

pub struct PagesIvars {
    file: PathBuf,
    /// Each page's size in points.
    sizes: Vec<NSSize>,
    /// Where each page is drawn, for the view's width.
    frames: RefCell<Vec<NSRect>>,
    /// The pages kept drawn (or being drawn), by index.
    images: RefCell<HashMap<usize, Rc<DeferredImage>>>,
    /// The page shown (index): the one at the top of the view, or the one jumped to.
    current: Cell<usize>,
    /// The page jumped to stays the one shown until the user scrolls (layout moving the view,
    /// or a last page that cannot reach the top, does not choose another).
    jumped: Cell<bool>,
    /// The scroll view's magnification the pages are drawn for.
    zoom: Cell<f64>,
    /// What shows of the pages.
    canvas: RefCell<Option<Retained<PdfCanvas>>>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesPdfPages"]
    #[ivars = PagesIvars]
    pub struct PdfPages;

    impl PdfPages {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// The width follows the scroll view; the height is the pages'.
        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let height = self.lay_out(size.width);
            // SAFETY: the superclass sets the frame's size.
            let _: () = unsafe { msg_send![super(self), setFrameSize: NSSize::new(size.width, height)] };
            self.follow();
        }

        /// The user scrolls: the page at the top is the one shown from now on.
        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.ivars().jumped.set(false);
            // SAFETY: forwarding the same event to the superclass implementation.
            let _: () = unsafe { msg_send![super(self), scrollWheel: event] };
        }

        /// The view scrolled: the canvas, the page shown and the pages kept follow.
        #[unsafe(method(boundsDidChange:))]
        fn bounds_did_change(&self, _notification: &NSNotification) {
            self.follow();
        }

        /// A zoom ended: the pages are drawn again, sharp at it.
        #[unsafe(method(magnifyEnded:))]
        fn magnify_ended(&self, _notification: &NSNotification) {
            if let Some(scroll) = self.enclosingScrollView() {
                self.ivars().zoom.set(scroll.magnification().max(1.0));
            }
            self.let_all_go();
            self.follow();
        }
    }
);

define_class!(
    /// The visible part of the pages, drawn.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesPdfCanvas"]
    #[ivars = ()]
    pub struct PdfCanvas;

    impl PdfCanvas {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty: NSRect) {
            // SAFETY: the canvas's own superview, on the main thread.
            let Some(pages) = unsafe { self.superview() }.and_then(|s| s.downcast::<PdfPages>().ok()) else { return };
            // The pages' coordinates: the canvas sits on the visible part.
            let at = self.frame().origin;
            let transform = NSAffineTransform::transform();
            transform.translateXBy_yBy(-at.x, -at.y);
            transform.concat();
            pages.draw_pages(NSRect::new(NSPoint::new(dirty.origin.x + at.x, dirty.origin.y + at.y), dirty.size));
        }
    }
);

impl PdfPages {
    /// Places the pages for `width`; gives the height they take.
    fn lay_out(&self, width: f64) -> f64 {
        let page_width = (width - 2.0 * MARGIN).max(1.0);
        let mut y = MARGIN;
        let frames: Vec<NSRect> = self
            .ivars()
            .sizes
            .iter()
            .map(|size| {
                let height = page_width * size.height / size.width.max(1.0);
                let frame = NSRect::new(NSPoint::new(MARGIN, y), NSSize::new(page_width, height));
                y += height + GAP;
                frame
            })
            .collect();
        let resized = self.ivars().frames.borrow().first().is_some_and(|f| (f.size.width - page_width).abs() > 0.5);
        *self.ivars().frames.borrow_mut() = frames;
        if resized {
            // Drawn for another width.
            self.let_all_go();
        }
        y - GAP + MARGIN
    }

    /// Draws the pages meeting `dirty` (the pages' coordinates) into the current context.
    fn draw_pages(&self, dirty: NSRect) {
        NSColor::underPageBackgroundColor().setFill();
        NSBezierPath::fillRect(dirty);
        let frames = self.ivars().frames.borrow();
        let images = self.ivars().images.borrow();
        for (i, frame) in frames.iter().enumerate().filter(|(_, f)| intersects(**f, dirty)) {
            NSColor::whiteColor().setFill();
            NSBezierPath::fillRect(*frame);
            if let Some(image) = images.get(&i).and_then(|image| image.get()) {
                // SAFETY: a valid image drawn into the current context.
                unsafe { image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(*frame, NSZeroRect, NSCompositingOperation::SourceOver, 1.0, true, None) };
            }
            NSColor::separatorColor().setStroke();
            NSBezierPath::strokeRect(*frame);
        }
    }

    /// The canvas onto the visible part, then the page shown and the pages kept.
    fn follow(&self) {
        let visible = self.visibleRect();
        if let Some(canvas) = self.ivars().canvas.borrow().as_ref() {
            canvas.setFrame(visible);
            canvas.setNeedsDisplay(true);
        }
        self.track();
    }

    /// Chooses the page shown (unless one was jumped to) and keeps drawn that page, two
    /// above and two below it and the pages in view; lets the others go.
    fn track(&self) {
        let frames = self.ivars().frames.borrow().clone();
        if frames.is_empty() {
            return;
        }
        let visible = self.visibleRect();
        let top = visible.origin.y + self.enclosingScrollView().map_or(0.0, |s| s.contentInsets().top) + 1.0;
        if !self.ivars().jumped.get() {
            let at_top = frames.iter().position(|f| f.origin.y + f.size.height + GAP > top).unwrap_or(frames.len() - 1);
            self.ivars().current.set(at_top);
        }
        let current = self.ivars().current.get();
        let in_view: Vec<usize> = (0..frames.len()).filter(|&i| intersects(frames[i], visible)).collect();
        let first = current.saturating_sub(KEPT_AROUND).min(in_view.first().copied().unwrap_or(usize::MAX));
        let last = (current + KEPT_AROUND).min(frames.len() - 1).max(in_view.last().copied().unwrap_or(0));
        let mut images = self.ivars().images.borrow_mut();
        images.retain(|&i, image| {
            let kept = (first..=last).contains(&i);
            if !kept {
                image.let_go();
            }
            kept
        });
        let zoom = self.ivars().zoom.get();
        for (i, frame) in frames.iter().enumerate().take(last + 1).skip(first) {
            images.entry(i).or_insert_with(|| {
                let size = NSSize::new(frame.size.width * zoom, frame.size.height * zoom);
                let this = objc2::rc::Weak::new(self);
                Rc::new(DeferredImage::new(self.ivars().file.clone(), size, Some(i as u32 + 1), move || {
                    // Drawn: the canvas shows it.
                    if let Some(canvas) = this.load().and_then(|this| this.ivars().canvas.borrow().clone()) {
                        canvas.setNeedsDisplay(true);
                    }
                }))
            });
        }
    }

    fn let_all_go(&self) {
        for image in self.ivars().images.borrow_mut().drain().map(|(_, image)| image) {
            image.let_go();
        }
        if let Some(canvas) = self.ivars().canvas.borrow().as_ref() {
            canvas.setNeedsDisplay(true);
        }
    }
}

/// The scroll view showing a PDF's pages.
pub struct PdfViewer {
    pub scroll: Retained<NSScrollView>,
    pages: Retained<PdfPages>,
}

impl PdfViewer {
    /// The PDF in `file` in a scroll view `frame` large; `None` when it does not open.
    pub fn new(mtm: MainThreadMarker, file: &Path, frame: NSRect) -> Option<Self> {
        let sizes = image_loader::pdf_page_sizes(file).filter(|s| !s.is_empty())?;
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setAllowsMagnification(true);
        scroll.setMinMagnification(0.5);
        scroll.setMaxMagnification(4.0);
        scroll.setBackgroundColor(&NSColor::underPageBackgroundColor());
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        let ivars = PagesIvars {
            file: file.to_path_buf(),
            sizes,
            frames: RefCell::new(Vec::new()),
            images: RefCell::new(HashMap::new()),
            current: Cell::new(0),
            jumped: Cell::new(false),
            zoom: Cell::new(1.0),
            canvas: RefCell::new(None),
        };
        let width = scroll.contentSize().width;
        let start = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, 0.0));
        // SAFETY: NSView's designated initializer.
        let pages: Retained<PdfPages> = unsafe { msg_send![super(PdfPages::alloc(mtm).set_ivars(ivars)), initWithFrame: start] };
        // SAFETY: as above.
        let canvas: Retained<PdfCanvas> = unsafe { msg_send![super(PdfCanvas::alloc(mtm).set_ivars(())), initWithFrame: start] };
        pages.addSubview(&canvas);
        *pages.ivars().canvas.borrow_mut() = Some(canvas);
        pages.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        pages.setFrameSize(NSSize::new(width, 0.0));
        scroll.setDocumentView(Some(&pages));
        let clip = scroll.contentView();
        clip.setPostsBoundsChangedNotifications(true);
        let observer: &AnyObject = pages.as_ref();
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: the observer is removed when the viewer goes; both selectors take a notification.
        unsafe {
            center.addObserver_selector_name_object(observer, sel!(boundsDidChange:), Some(NSViewBoundsDidChangeNotification), Some(clip.as_ref()));
            center.addObserver_selector_name_object(observer, sel!(magnifyEnded:), Some(NSScrollViewDidEndLiveMagnifyNotification), Some(scroll.as_ref()));
        }
        Some(Self { scroll, pages })
    }

    /// Shows page `page` (index) at the top.
    pub fn go_to(&self, page: usize) {
        let ivars = self.pages.ivars();
        let Some(frame) = ivars.frames.borrow().get(page).copied() else { return };
        ivars.jumped.set(true);
        ivars.current.set(page);
        // Laid out first: the content insets under the toolbar come with layout.
        self.scroll.layoutSubtreeIfNeeded();
        let clip = self.scroll.contentView();
        clip.scrollToPoint(NSPoint::new(0.0, frame.origin.y - MARGIN - self.scroll.contentInsets().top));
        self.scroll.reflectScrolledClipView(&clip);
        self.pages.follow();
    }

    /// The page shown (from 1) and how many there are.
    pub fn page(&self) -> (usize, usize) {
        (self.pages.ivars().current.get() + 1, self.pages.ivars().sizes.len())
    }

    /// The pages kept drawn (from 1), in order (probe mode).
    pub fn kept(&self) -> Vec<usize> {
        let mut kept: Vec<usize> = self.pages.ivars().images.borrow().keys().map(|i| i + 1).collect();
        kept.sort_unstable();
        kept
    }
}

impl Drop for PdfViewer {
    fn drop(&mut self) {
        let observer: &AnyObject = self.pages.as_ref();
        // SAFETY: removes the registrations `new` made.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(observer) };
        self.pages.let_all_go();
        self.scroll.removeFromSuperview();
    }
}

fn intersects(a: NSRect, b: NSRect) -> bool {
    a.origin.x < b.origin.x + b.size.width && b.origin.x < a.origin.x + a.size.width && a.origin.y < b.origin.y + b.size.height && b.origin.y < a.origin.y + a.size.height
}
