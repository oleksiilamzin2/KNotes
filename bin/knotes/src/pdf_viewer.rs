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
//!
//! Text is selected with the mouse and copied: a page's text and its characters' boxes come
//! from PDFKit when the page is clicked or drawn selected, and nothing of PDFKit's is kept.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use knotes_pdf::PageText;
use objc2_app_kit::{
    NSAffineTransformNSAppKitAdditions, NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSCompositingOperation, NSCursor, NSEvent, NSEventModifierFlags, NSImage, NSMenu, NSMenuItem, NSPasteboard,
    NSPasteboardTypeString, NSRectFillUsingOperation, NSResponder, NSScrollView, NSScrollViewDidEndLiveMagnifyNotification, NSView, NSViewBoundsDidChangeNotification,
};
use objc2_foundation::{NSAffineTransform, NSNotification, NSNotificationCenter, NSObject, NSPoint, NSRect, NSSize, NSString, NSZeroRect};

use crate::strings;

use crate::image_loader::{self, DeferredImage};

/// Space around and between pages.
const MARGIN: f64 = 16.0;
const GAP: f64 = 12.0;
/// Pages kept drawn above and below the one shown.
const KEPT_AROUND: usize = 2;
/// How long the width stays put before the pages are drawn again for it.
const REDRAW_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

/// A place in the text: a page (index) and a caret on it.
type Spot = (usize, usize);

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
    /// Pages' text read so far, by index.
    texts: RefCell<HashMap<usize, Rc<PageText>>>,
    /// The selection: where it started and where it ends now.
    selection: Cell<Option<(Spot, Spot)>>,
    /// Called after the pages scrolled.
    on_scroll: RefCell<Option<Box<dyn Fn()>>>,
    /// Pages as drawn for the width or zoom before, shown stretched until drawn again (a gap
    /// between them flickered as the width changed).
    stand_ins: RefCell<HashMap<usize, Retained<NSImage>>>,
    /// Counts width changes: the pages are drawn again once it stops changing.
    resizes: Cell<u64>,
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

        /// The width follows the scroll view; the height is the pages'. A new width keeps the
        /// place at the top where it is, the pages stretched until the width settles.
        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let anchor = self.anchor_at_top();
            let (height, resized) = self.lay_out(size.width);
            // SAFETY: the superclass sets the frame's size.
            let _: () = unsafe { msg_send![super(self), setFrameSize: NSSize::new(size.width, height)] };
            if resized {
                if let Some(anchor) = anchor {
                    self.keep_at_top(anchor);
                }
                self.redraw_later();
            }
            self.follow();
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// A click puts the caret, a double click selects a word, a triple click a line; with
        /// Shift the selection extends to the click.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(window) = self.window() {
                window.makeFirstResponder(Some(self));
            }
            self.press(event);
        }

        /// Dragging selects from where the press was.
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.autoscroll(event);
            let ivars = self.ivars();
            if let (Some((anchor, _)), Some(head)) = (ivars.selection.get(), self.spot_at(event)) {
                ivars.selection.set(Some((anchor, head)));
                self.selection_changed();
            }
        }

        /// Edit > Copy (Cmd+C).
        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            self.copy_to(&NSPasteboard::generalPasteboard());
        }

        /// Copy is on with text selected.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            item.action() != Some(sel!(copy:)) || self.ordered().is_some()
        }

        /// A right click with text selected offers to copy it.
        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, _event: &NSEvent) -> Option<Retained<NSMenu>> {
            if self.ordered().is_none() {
                None
            } else {
                let menu = NSMenu::new(self.mtm());
                // SAFETY: an item sent up the responder chain; the title and key are strings.
                let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm()), &NSString::from_str(strings::COPY), Some(sel!(copy:)), &NSString::new()) };
                menu.addItem(&item);
                Some(menu)
            }
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
            self.redraw_all();
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

        /// The pages under it take the mouse.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
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
    /// Places the pages for `width`; gives the height they take and whether their width changed.
    fn lay_out(&self, width: f64) -> (f64, bool) {
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
        (y - GAP + MARGIN, resized)
    }

    /// The place at the top of the view, under the toolbar: a page (index) and how far down it
    /// (a fraction), or how many points above it, in the space before it.
    fn anchor_at_top(&self) -> Option<(usize, f64, bool)> {
        let frames = self.ivars().frames.borrow();
        let y = self.visibleRect().origin.y + self.enclosingScrollView().map_or(0.0, |s| s.contentInsets().top);
        let page = frames.iter().position(|f| f.origin.y + f.size.height > y).or(frames.len().checked_sub(1))?;
        let f = frames[page];
        Some(if y < f.origin.y { (page, y - f.origin.y, false) } else { (page, (y - f.origin.y) / f.size.height, true) })
    }

    /// Scrolls the place `anchor_at_top` gave back to the top.
    fn keep_at_top(&self, (page, at, fraction): (usize, f64, bool)) {
        let Some(f) = self.ivars().frames.borrow().get(page).copied() else { return };
        let Some(scroll) = self.enclosingScrollView() else { return };
        let y = f.origin.y + if fraction { at * f.size.height } else { at };
        let clip = scroll.contentView();
        clip.scrollToPoint(NSPoint::new(clip.bounds().origin.x, y - scroll.contentInsets().top));
        scroll.reflectScrolledClipView(&clip);
    }

    /// Draws the pages again for the new width once it has not changed for a moment: while
    /// a divider or the window is dragged, they only stretch (drawing every step made them
    /// blur and sharpen over and over).
    fn redraw_later(&self) {
        let resize = self.ivars().resizes.get() + 1;
        self.ivars().resizes.set(resize);
        let target = dispatch2::MainThreadBound::new(objc2::rc::Weak::new(self), self.mtm());
        let when = dispatch2::DispatchTime::try_from(REDRAW_DELAY).expect("short delay");
        let _ = dispatch2::DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(this) = target.get(mtm).load()
                && this.ivars().resizes.get() == resize
            {
                this.redraw_all();
                this.follow();
            }
        });
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
            // The page's text read only when something on it is selected.
            if let Some((range, text)) = self.selected_on(i).and_then(|range| Some((range, self.text_of(i)?))) {
                NSColor::selectedTextBackgroundColor().setFill();
                for b in text.highlight(range) {
                    let at = NSPoint::new(frame.origin.x + b.x * frame.size.width, frame.origin.y + b.y * frame.size.height);
                    // Multiplied, the text stays readable under it.
                    NSRectFillUsingOperation(NSRect::new(at, NSSize::new(b.width * frame.size.width, b.height * frame.size.height)), NSCompositingOperation::Multiply);
                }
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
        if let Some(callback) = self.ivars().on_scroll.borrow().as_ref() {
            callback();
        }
    }

    /// Chooses the page shown (unless one was jumped to) and keeps drawn that page, two
    /// above and two below it and the pages in view; lets the others go, and the text of pages
    /// neither in view nor selected.
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
        let mut stand_ins = self.ivars().stand_ins.borrow_mut();
        for (i, frame) in frames.iter().enumerate().take(last + 1).skip(first) {
            images.entry(i).or_insert_with(|| {
                let size = NSSize::new(frame.size.width * zoom, frame.size.height * zoom);
                let this = objc2::rc::Weak::new(self);
                let image = DeferredImage::new(self.ivars().file.clone(), size, Some(i as u32 + 1), move || {
                    // Drawn: the canvas shows it.
                    if let Some(canvas) = this.load().and_then(|this| this.ivars().canvas.borrow().clone()) {
                        canvas.setNeedsDisplay(true);
                    }
                });
                Rc::new(image.standing_in(stand_ins.remove(&i)))
            });
        }
        // Pages no longer kept need none.
        stand_ins.clear();
        drop(stand_ins);
        drop(images);
        self.keep_texts();
    }

    /// The page at `point` (the pages' coordinates; the nearest page between them) and the
    /// point's place on it, as fractions of its width and height.
    fn place_at(&self, point: NSPoint) -> Option<(usize, f64, f64)> {
        let frames = self.ivars().frames.borrow();
        let page = frames.iter().position(|f| point.y < f.origin.y + f.size.height + GAP / 2.0).unwrap_or(frames.len().checked_sub(1)?);
        let f = frames[page];
        Some((page, (point.x - f.origin.x) / f.size.width, (point.y - f.origin.y) / f.size.height))
    }

    /// The place in the text under the mouse of `event`.
    fn spot_at(&self, event: &NSEvent) -> Option<Spot> {
        let (page, x, y) = self.place_at(self.convertPoint_fromView(event.locationInWindow(), None))?;
        Some((page, self.text_of(page).map_or(0, |t| t.caret_at(x, y))))
    }

    fn press(&self, event: &NSEvent) {
        let Some((page, x, y)) = self.place_at(self.convertPoint_fromView(event.locationInWindow(), None)) else { return };
        let text = self.text_of(page);
        let ivars = self.ivars();
        let around = |range: Range<usize>| Some(((page, range.start), (page, range.end)));
        let selection = match (event.clickCount(), text) {
            (2, Some(text)) => around(text.word_at(x, y)),
            (3.., Some(text)) => around(text.line_at(x, y)),
            (_, text) => {
                let spot = (page, text.map_or(0, |t| t.caret_at(x, y)));
                let anchor = ivars.selection.get().filter(|_| event.modifierFlags().contains(NSEventModifierFlags::Shift)).map_or(spot, |(anchor, _)| anchor);
                Some((anchor, spot))
            }
        };
        ivars.selection.set(selection);
        self.selection_changed();
    }

    fn selection_changed(&self) {
        if let Some(canvas) = self.ivars().canvas.borrow().as_ref() {
            canvas.setNeedsDisplay(true);
        }
        self.keep_texts();
    }

    /// Keeps the text of the pages in view and of the pages selected; lets the others go.
    fn keep_texts(&self) {
        let visible = self.visibleRect();
        let selected = self.ordered().map(|((first, _), (last, _))| first..=last);
        let frames = self.ivars().frames.borrow();
        self.ivars().texts.borrow_mut().retain(|&i, _| frames.get(i).is_some_and(|f| intersects(*f, visible)) || selected.as_ref().is_some_and(|s| s.contains(&i)));
    }

    /// The selection in order, when it holds anything.
    fn ordered(&self) -> Option<(Spot, Spot)> {
        let (a, b) = self.ivars().selection.get()?;
        (a != b).then_some((a.min(b), a.max(b)))
    }

    /// What is selected on page `page`.
    fn selected_on(&self, page: usize) -> Option<Range<usize>> {
        let ((first, start), (last, end)) = self.ordered()?;
        (first..=last).contains(&page).then_some(if page == first { start } else { 0 }..if page == last { end } else { usize::MAX })
    }

    /// The selected text, pages joined by line breaks.
    fn selected_text(&self) -> String {
        let Some(((first, _), (last, _))) = self.ordered() else { return String::new() };
        (first..=last).filter_map(|page| Some(self.text_of(page)?.text(self.selected_on(page)?))).collect::<Vec<_>>().join("\n")
    }

    /// Writes the selected text to `pasteboard`; nothing when none is.
    fn copy_to(&self, pasteboard: &NSPasteboard) {
        let text = self.selected_text();
        if !text.is_empty() {
            pasteboard.clearContents();
            // SAFETY: AppKit's pasteboard type constant lives for the whole process.
            pasteboard.setString_forType(&NSString::from_str(&text), unsafe { NSPasteboardTypeString });
        }
    }

    /// Page `page`'s text, read from the PDF the first time it is asked for.
    fn text_of(&self, page: usize) -> Option<Rc<PageText>> {
        if let Some(text) = self.ivars().texts.borrow().get(&page) {
            return Some(text.clone());
        }
        let text = Rc::new(knotes_pdf::page_text(&self.ivars().file, page)?);
        self.ivars().texts.borrow_mut().insert(page, text.clone());
        Some(text)
    }

    /// Draws every page again (for a new width or zoom), each shown as it was, stretched,
    /// until it is.
    fn redraw_all(&self) {
        let old: Vec<(usize, Rc<DeferredImage>)> = self.ivars().images.borrow_mut().drain().collect();
        let mut stand_ins = self.ivars().stand_ins.borrow_mut();
        for (i, image) in old {
            if let Some(drawn) = image.ready() {
                stand_ins.insert(i, drawn);
            }
            image.let_go();
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
        scroll.setDocumentCursor(Some(&NSCursor::IBeamCursor()));
        let ivars = PagesIvars {
            file: file.to_path_buf(),
            sizes,
            frames: RefCell::new(Vec::new()),
            images: RefCell::new(HashMap::new()),
            current: Cell::new(0),
            jumped: Cell::new(false),
            zoom: Cell::new(1.0),
            canvas: RefCell::new(None),
            texts: RefCell::new(HashMap::new()),
            selection: Cell::new(None),
            on_scroll: RefCell::new(None),
            stand_ins: RefCell::new(HashMap::new()),
            resizes: Cell::new(0),
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
        self.go_to_place(page, 0.0);
    }

    /// Shows the point `y` (a fraction) down page `page` (index) at the top; the page's top
    /// with the margin above it.
    pub fn go_to_place(&self, page: usize, y: f64) {
        let ivars = self.pages.ivars();
        let Some(frame) = ivars.frames.borrow().get(page).copied() else { return };
        ivars.jumped.set(true);
        ivars.current.set(page);
        // Laid out first: the content insets under the toolbar come with layout.
        self.scroll.layoutSubtreeIfNeeded();
        let clip = self.scroll.contentView();
        let top = if y > 0.0 { frame.origin.y + y * frame.size.height } else { frame.origin.y - MARGIN };
        clip.scrollToPoint(NSPoint::new(0.0, top - self.scroll.contentInsets().top));
        self.scroll.reflectScrolledClipView(&clip);
        self.pages.follow();
    }

    /// Called after the pages scrolled.
    pub fn on_scroll(&self, callback: impl Fn() + 'static) {
        *self.pages.ivars().on_scroll.borrow_mut() = Some(Box::new(callback));
    }

    /// How far the pages are scrolled.
    pub fn scroll_y(&self) -> f64 {
        self.scroll.contentView().bounds().origin.y
    }

    /// The place at the top of the view, under the toolbar and the margin above a page: the
    /// page there (index) and how far down it (a fraction).
    pub fn top_place(&self) -> (usize, f64) {
        let frames = self.pages.ivars().frames.borrow();
        let y = self.scroll_y() + self.scroll.contentInsets().top + MARGIN + 1.0;
        let page = frames.iter().position(|f| f.origin.y + f.size.height > y).unwrap_or(frames.len().saturating_sub(1));
        let fraction = frames.get(page).map_or(0.0, |f| ((y - f.origin.y) / f.size.height).clamp(0.0, 1.0));
        (page, fraction)
    }

    /// Where the pages are: the page shown (index) and how far down it the top of the view is,
    /// under the toolbar (a fraction; below nothing in the margin above it).
    pub fn place(&self) -> (usize, f64) {
        let ivars = self.pages.ivars();
        let page = ivars.current.get();
        let Some(f) = ivars.frames.borrow().get(page).copied() else { return (0, 0.0) };
        let y = self.scroll_y() + self.scroll.contentInsets().top;
        (page, (y - f.origin.y) / f.size.height)
    }

    /// Shows the pages where `place` says.
    pub fn show_place(&self, (page, at): (usize, f64)) {
        let ivars = self.pages.ivars();
        if page >= ivars.frames.borrow().len() {
            return;
        }
        ivars.jumped.set(true);
        ivars.current.set(page);
        self.scroll.layoutSubtreeIfNeeded();
        self.pages.keep_at_top((page, at, true));
        self.pages.follow();
    }

    /// The page shown (from 1) and how many there are.
    pub fn page(&self) -> (usize, usize) {
        (self.pages.ivars().current.get() + 1, self.pages.ivars().sizes.len())
    }

    /// The selected text (probe mode).
    pub fn selected_text(&self) -> String {
        self.pages.selected_text()
    }

    /// Writes the selected text to `pasteboard` (probe mode: one of its own).
    pub fn copy_to(&self, pasteboard: &NSPasteboard) {
        self.pages.copy_to(pasteboard);
    }

    /// The view taking the mouse (probe mode).
    pub fn view(&self) -> &NSView {
        &self.pages
    }

    /// Where `x`, `y` (fractions of the page) on page `page` (index) is in the window (probe
    /// mode).
    pub fn window_point(&self, page: usize, x: f64, y: f64) -> Option<NSPoint> {
        let f = self.pages.ivars().frames.borrow().get(page).copied()?;
        Some(self.pages.convertPoint_toView(NSPoint::new(f.origin.x + x * f.size.width, f.origin.y + y * f.size.height), None))
    }

    /// The pages whose text is kept (from 1), in order (probe mode).
    pub fn texts_kept(&self) -> Vec<usize> {
        let mut kept: Vec<usize> = self.pages.ivars().texts.borrow().keys().map(|i| i + 1).collect();
        kept.sort_unstable();
        kept
    }

    /// What each page kept shows and the width it is drawn for, `1:ready:568` (from 1), in
    /// order (probe mode).
    pub fn drawn(&self) -> Vec<String> {
        let images = self.pages.ivars().images.borrow();
        let mut pages: Vec<(usize, &'static str, f64)> = images.iter().map(|(i, image)| (i + 1, image.shows(), image.size.width)).collect();
        pages.sort_by_key(|p| p.0);
        pages.iter().map(|(page, shows, width)| format!("{page}:{shows}:{width:.0}")).collect()
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
