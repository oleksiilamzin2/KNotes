//! The attachments' window: every file of the vault that is no note (an image, a PDF, any
//! other) with the links to it, those no link points at, or the links to images and PDFs that
//! are not there. A file is shown in Finder or deleted (into the recently deleted); a link
//! with no file opens its note. The vault's window reads the lists and does what is asked.

use std::cell::{Cell, RefCell};

use knotes_app::{AttachmentFilter, AttachmentItem, MarkedRows, attachment_items};
use knotes_core::{Attachment, MissingAttachment};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSBox, NSBoxType, NSButton, NSColor, NSEvent, NSEventModifierFlags, NSFont, NSFontWeightSemibold, NSResponder, NSScrollView, NSSegmentedControl,
    NSSegmentSwitchTracking, NSStringDrawing, NSTextField, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowStyleMask, NSImage, NSImageView, NSPopover, NSPopoverBehavior, NSViewController,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSSize, NSString};

use crate::search_window::{attributes, fitted, rect};
use crate::strings;

const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 500.0;
/// The filter's bar on top and the buttons' at the bottom.
const BAR: f64 = 48.0;
const ROW: f64 = 44.0;
/// The longest side of a preview.
const PREVIEW: f64 = 360.0;
const FILTERS: [AttachmentFilter; 3] = [AttachmentFilter::All, AttachmentFilter::Unlinked, AttachmentFilter::Missing];

/// How a row is picked: alone, with ⌘ (one more or one less), with ⇧ (all up to it).
#[derive(Clone, Copy)]
enum Pick {
    Row,
    Toggle,
    Extend,
}

pub struct RowsIvars {
    items: RefCell<Vec<AttachmentItem>>,
    chosen: Cell<Option<usize>>,
    marked: RefCell<MarkedRows>,
    on_change: RefCell<Option<Box<dyn Fn()>>>,
    on_open: RefCell<Option<Box<dyn Fn(usize)>>>,
    on_delete: RefCell<Option<Box<dyn Fn()>>>,
    /// Called with the row the pointer is over, or none: its preview shows.
    on_hover: RefCell<Option<Box<dyn Fn(Option<usize>)>>>,
    hovered: Cell<Option<usize>>,
    /// The tracking area that reports the pointer's moves over the rows.
    moves: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    /// The attachments listed, a row each: a file's name over where it is, its size and the
    /// links to it; a missing one's name over the note that links to it.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesAttachmentRows"]
    #[ivars = RowsIvars]
    pub struct AttachmentRows;

    impl AttachmentRows {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: objc2_foundation::NSRect) {
            let width = self.bounds().size.width;
            let (name_font, details_font) = (NSFont::systemFontOfSize_weight(13.0, unsafe { NSFontWeightSemibold }), NSFont::systemFontOfSize(11.0));
            let (ink, quiet) = (NSColor::labelColor(), NSColor::secondaryLabelColor());
            let chosen = self.chosen();
            for (at, item) in self.ivars().items.borrow().iter().enumerate() {
                let top = at as f64 * ROW;
                if chosen.contains(&at) {
                    NSColor::selectedContentBackgroundColor().colorWithAlphaComponent(0.25).setFill();
                    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(8.0, top + 2.0, width - 16.0, ROW - 4.0), 6.0, 6.0).fill();
                }
                let (name, details) = said(item);
                // SAFETY: a font and a colour are those attributes' values.
                unsafe {
                    NSString::from_str(&fitted(&name, &name_font, width - 40.0)).drawAtPoint_withAttributes(NSPoint::new(20.0, top + 6.0), Some(&attributes(&name_font, &ink)));
                    NSString::from_str(&fitted(&details, &details_font, width - 40.0)).drawAtPoint_withAttributes(NSPoint::new(20.0, top + 24.0), Some(&attributes(&details_font, &quiet)));
                }
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // SAFETY: the superclass keeps its own areas up to date.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
            if self.ivars().moves.borrow().is_none() {
                let options = NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect;
                // SAFETY: the view owns the area and outlives it.
                let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(NSTrackingArea::alloc(), objc2_foundation::NSRect::ZERO, options, Some(self), None) };
                self.addTrackingArea(&area);
                *self.ivars().moves.borrow_mut() = Some(area);
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let row = (at.y / ROW) as usize;
            self.hover((at.y >= 0.0 && row < self.ivars().items.borrow().len()).then_some(row));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.hover(None);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let row = (at.y / ROW) as usize;
            if at.y < 0.0 || row >= self.ivars().items.borrow().len() {
                return;
            }
            let flags = event.modifierFlags();
            let pick = if flags.contains(NSEventModifierFlags::Command) {
                Pick::Toggle
            } else if flags.contains(NSEventModifierFlags::Shift) {
                Pick::Extend
            } else {
                Pick::Row
            };
            self.pick(row, pick);
            if event.clickCount() == 2 && let Some(callback) = self.ivars().on_open.borrow().as_ref() {
                callback(row);
            }
        }

        /// The arrows move the choice (with ⇧, widen it); Delete deletes what is chosen.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let len = self.ivars().items.borrow().len();
            let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
            match (event.keyCode(), self.ivars().chosen.get()) {
                (125 | 126, Some(chosen)) if shift => {
                    let to = self.ivars().marked.borrow_mut().step(chosen, event.keyCode() == 125, len);
                    self.changed(to);
                }
                (125, chosen) if len > 0 => self.pick(chosen.map_or(0, |row| (row + 1).min(len - 1)), Pick::Row),
                (126, Some(chosen)) => self.pick(chosen.saturating_sub(1), Pick::Row),
                (51 | 117, Some(_)) => {
                    if let Some(callback) = self.ivars().on_delete.borrow().as_ref() {
                        callback();
                    }
                }
                // SAFETY: the same event, to the superclass.
                _ => unsafe { msg_send![super(self), keyDown: event] },
            }
        }
    }
);

impl AttachmentRows {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = RowsIvars {
            items: RefCell::new(Vec::new()),
            chosen: Cell::new(None),
            marked: RefCell::new(MarkedRows::default()),
            on_change: RefCell::new(None),
            on_open: RefCell::new(None),
            on_delete: RefCell::new(None),
            on_hover: RefCell::new(None),
            hovered: Cell::new(None),
            moves: RefCell::new(None),
        };
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(ivars)), init] }
    }

    /// The pointer is over `row` now (or over none): told once per row.
    fn hover(&self, row: Option<usize>) {
        if self.ivars().hovered.replace(row) != row
            && let Some(callback) = self.ivars().on_hover.borrow().as_ref()
        {
            callback(row);
        }
    }

    /// The rows chosen, from the top.
    fn chosen(&self) -> Vec<usize> {
        self.ivars().marked.borrow().with(self.ivars().chosen.get())
    }

    fn pick(&self, row: usize, pick: Pick) {
        let chosen = self.ivars().chosen.get();
        let mut marked = self.ivars().marked.borrow_mut();
        match (pick, chosen) {
            (Pick::Toggle, _) => self.ivars().chosen.set(Some(marked.toggle(chosen, row))),
            (Pick::Extend, Some(chosen)) => marked.extend(chosen, row),
            _ => {
                marked.clear();
                self.ivars().chosen.set(Some(row));
            }
        }
        drop(marked);
        self.changed(row);
    }

    /// The choice changed: drawn again, the row `row` in view, the window's buttons told.
    fn changed(&self, row: usize) {
        self.setNeedsDisplay(true);
        self.scrollRectToVisible(rect(0.0, row as f64 * ROW, 1.0, ROW));
        if let Some(callback) = self.ivars().on_change.borrow().as_ref() {
            callback();
        }
    }

    fn set_items(&self, items: Vec<AttachmentItem>) {
        self.setFrameSize(NSSize::new(WIDTH, (items.len() as f64 * ROW).max(HEIGHT - 2.0 * BAR)));
        *self.ivars().items.borrow_mut() = items;
        self.hover(None);
        self.ivars().chosen.set(None);
        self.ivars().marked.borrow_mut().clear();
        self.setNeedsDisplay(true);
    }
}

/// A row's two lines.
fn said(item: &AttachmentItem) -> (String, String) {
    match item {
        AttachmentItem::File(file) => {
            let (folder, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
            (name.to_owned(), strings::attachment_details(folder, file.size, file.links))
        }
        AttachmentItem::Missing(missing) => (missing.target.clone(), strings::attachment_missing_in(knotes_app::title_of(&missing.source))),
    }
}

/// What the window asks of the vault's window.
pub struct Asks {
    /// The files at these vault paths deleted, once confirmed (the second: how many of them
    /// have links to them); true when they were.
    pub delete: Box<dyn Fn(&[String], usize) -> bool>,
    /// The files at these vault paths shown in Finder.
    pub reveal: Box<dyn Fn(&[String])>,
    /// Where the file at this vault path is on the disk: its preview is read from there.
    pub file: Box<dyn Fn(&str) -> Option<std::path::PathBuf>>,
    /// The file or the note at this vault path opened.
    pub open: Box<dyn Fn(&str)>,
}

pub struct Ivars {
    window: Retained<NSWindow>,
    rows: Retained<AttachmentRows>,
    filters: Retained<NSSegmentedControl>,
    summary: Retained<NSTextField>,
    reveal: Retained<NSButton>,
    act: Retained<NSButton>,
    filter: Cell<AttachmentFilter>,
    files: RefCell<Vec<Attachment>>,
    missing: RefCell<Vec<MissingAttachment>>,
    asks: RefCell<Option<Asks>>,
    /// The preview of the file under the pointer, beside the window, and what it shows:
    /// "pic.png 120x80".
    preview: Retained<NSPopover>,
    picture: Retained<NSImageView>,
    previewed: RefCell<Option<String>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesAttachments"]
    #[ivars = Ivars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    impl Controller {
        #[unsafe(method(filterChanged:))]
        fn filter_changed(&self, sender: &NSSegmentedControl) {
            self.set_filter(FILTERS[(sender.selectedSegment().max(0) as usize).min(2)]);
        }

        #[unsafe(method(revealChosen:))]
        fn reveal_chosen(&self, _sender: Option<&AnyObject>) {
            let paths = self.chosen_files().into_iter().map(|file| file.path).collect::<Vec<_>>();
            if !paths.is_empty() {
                self.ask(|asks| (asks.reveal)(&paths));
            }
        }

        /// The button on the right: the files chosen deleted, or a missing one's note opened.
        #[unsafe(method(actOnChosen:))]
        fn act_on_chosen(&self, _sender: Option<&AnyObject>) {
            if self.ivars().filter.get() == AttachmentFilter::Missing {
                if let Some(row) = self.ivars().rows.ivars().chosen.get() {
                    self.open_row(row);
                }
            } else {
                self.delete_chosen();
            }
        }
    }
);

impl Controller {
    fn ask(&self, what: impl FnOnce(&Asks)) {
        if let Some(asks) = self.ivars().asks.borrow().as_ref() {
            what(asks);
        }
    }

    fn chosen_files(&self) -> Vec<Attachment> {
        let rows = &self.ivars().rows;
        let items = rows.ivars().items.borrow();
        rows.chosen()
            .into_iter()
            .filter_map(|row| match items.get(row) {
                Some(AttachmentItem::File(file)) => Some(file.clone()),
                _ => None,
            })
            .collect()
    }

    /// The files chosen deleted once the vault's window says so: off the lists at once (the
    /// index hears of it a moment later).
    fn delete_chosen(&self) {
        let files = self.chosen_files();
        if files.is_empty() {
            return;
        }
        let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        let linked = files.iter().filter(|file| file.links > 0).count();
        let mut deleted = false;
        self.ask(|asks| deleted = (asks.delete)(&paths, linked));
        if deleted {
            self.ivars().files.borrow_mut().retain(|file| !paths.contains(&file.path));
            self.list();
        }
    }

    /// A double click (or the button, for a missing one): a file opened, a missing one's note.
    fn open_row(&self, row: usize) {
        let path = match self.ivars().rows.ivars().items.borrow().get(row) {
            Some(AttachmentItem::File(file)) => file.path.clone(),
            Some(AttachmentItem::Missing(missing)) => missing.source.clone(),
            None => return,
        };
        self.ask(|asks| (asks.open)(&path));
    }

    /// The pointer came over `row` (or left the rows): an image's or a PDF's picture shows
    /// beside the window, no larger than `PREVIEW`; another file's, a missing one's: none.
    fn preview(&self, row: Option<usize>) {
        let ivars = self.ivars();
        let path = row.and_then(|row| match ivars.rows.ivars().items.borrow().get(row) {
            Some(AttachmentItem::File(file)) if matches!(knotes_core::FileKind::from_path(&file.path), knotes_core::FileKind::Image | knotes_core::FileKind::Pdf) => Some(file.path.clone()),
            _ => None,
        });
        let mut file = None;
        if let Some(path) = &path {
            self.ask(|asks| file = (asks.file)(path));
        }
        // shortcut: read whole on the main thread at each hover (a 12 MP photo: tens of ms); decode a thumbnail off it if that is felt.
        let image = file.and_then(|file| NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&file.to_string_lossy())));
        let size = image.as_ref().map(|image| image.size()).filter(|size| size.width > 0.0 && size.height > 0.0);
        let (Some(row), Some(path), Some(image), Some(size)) = (row, path, image, size) else {
            *ivars.previewed.borrow_mut() = None;
            ivars.preview.close();
            return;
        };
        let scale = (PREVIEW / size.width).min(PREVIEW / size.height).min(1.0);
        let shown = NSSize::new((size.width * scale).round(), (size.height * scale).round());
        *ivars.previewed.borrow_mut() = Some(format!("{} {:.0}x{:.0}", path.rsplit('/').next().unwrap_or(&path), shown.width, shown.height));
        ivars.picture.setImage(Some(&image));
        ivars.preview.setContentSize(shown);
        // In probe mode nothing comes on the screen.
        if !crate::probe::enabled() {
            let width = ivars.rows.bounds().size.width;
            ivars.preview.showRelativeToRect_ofView_preferredEdge(rect(width - 24.0, row as f64 * ROW, 24.0, ROW), &ivars.rows, objc2_foundation::NSRectEdge::MaxX);
        }
    }

    fn set_filter(&self, filter: AttachmentFilter) {
        self.ivars().filter.set(filter);
        self.list();
    }

    /// The rows of the filter shown, the counts in the filter's segments, the summary.
    fn list(&self) {
        let ivars = self.ivars();
        let (files, missing) = (ivars.files.borrow(), ivars.missing.borrow());
        let filter = ivars.filter.get();
        ivars.rows.set_items(attachment_items(&files, &missing, filter));
        let unlinked = files.iter().filter(|file| file.links == 0).count();
        for (at, (title, count)) in [(strings::ATTACHMENTS_ALL, files.len()), (strings::ATTACHMENTS_UNLINKED, unlinked), (strings::ATTACHMENTS_MISSING, missing.len())].into_iter().enumerate() {
            ivars.filters.setLabel_forSegment(&NSString::from_str(&format!("{title} · {count}")), at as isize);
        }
        ivars.filters.setSelectedSegment(FILTERS.iter().position(|f| *f == filter).unwrap_or(0) as isize);
        let shown: Vec<&Attachment> = files.iter().filter(|file| filter != AttachmentFilter::Unlinked || file.links == 0).collect();
        let summary = if filter == AttachmentFilter::Missing { strings::attachments_missing_summary(missing.len()) } else { strings::attachments_summary(shown.len(), shown.iter().map(|file| file.size).sum()) };
        ivars.summary.setStringValue(&NSString::from_str(&summary));
        ivars.act.setTitle(&NSString::from_str(if filter == AttachmentFilter::Missing { strings::ATTACHMENT_OPEN_NOTE } else { strings::ATTACHMENT_DELETE }));
        drop((files, missing));
        self.buttons();
    }

    /// The buttons work while something they act on is chosen.
    fn buttons(&self) {
        let ivars = self.ivars();
        let missing = ivars.filter.get() == AttachmentFilter::Missing;
        let any = !ivars.rows.chosen().is_empty();
        ivars.reveal.setEnabled(any && !missing);
        ivars.act.setEnabled(any);
    }
}

/// The attachments' window.
pub struct AttachmentsWindow {
    controller: Retained<Controller>,
}

impl AttachmentsWindow {
    pub fn new(mtm: MainThreadMarker, asks: Asks) -> Self {
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
        // SAFETY: a window made and kept by Rust (`setReleasedWhenClosed(false)`).
        let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT), style, NSBackingStoreType::Buffered, false) };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(strings::ATTACHMENTS));
        crate::probe::conceal(&window);
        let content = window.contentView().expect("a window has a content view");
        let rows = AttachmentRows::new(mtm);
        // Frames are from the bottom up: the buttons' bar, the list, the filter's bar.
        let labels = NSArray::from_retained_slice(&[strings::ATTACHMENTS_ALL, strings::ATTACHMENTS_UNLINKED, strings::ATTACHMENTS_MISSING].map(NSString::from_str));
        // SAFETY: the target and the action are set once the controller is made.
        let filters = unsafe { NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(&labels, NSSegmentSwitchTracking::SelectOne, None, None, mtm) };
        filters.setFrame(rect(16.0, HEIGHT - BAR + 10.0, WIDTH - 32.0, 28.0));
        content.addSubview(&filters);
        let scroll = NSScrollView::new(mtm);
        scroll.setFrame(rect(0.0, BAR, WIDTH, HEIGHT - 2.0 * BAR));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(&rows));
        content.addSubview(&scroll);
        for y in [BAR, HEIGHT - BAR] {
            let line = NSBox::new(mtm);
            line.setBoxType(NSBoxType::Separator);
            line.setFrame(rect(0.0, y, WIDTH, 1.0));
            content.addSubview(&line);
        }
        let summary = NSTextField::labelWithString(&NSString::new(), mtm);
        summary.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        summary.setTextColor(Some(&NSColor::secondaryLabelColor()));
        summary.setFrame(rect(20.0, 15.0, WIDTH - 420.0, 18.0));
        content.addSubview(&summary);
        let button = |title: &str, x: f64, width: f64| {
            // SAFETY: as the filter's.
            let button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm) };
            button.setFrame(rect(x, 9.0, width, 30.0));
            content.addSubview(&button);
            button
        };
        let reveal = button(strings::VAULT_REVEAL, WIDTH - 376.0, 170.0);
        let act = button(strings::ATTACHMENT_DELETE, WIDTH - 196.0, 180.0);
        let picture = NSImageView::new(mtm);
        picture.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
        let shows = NSViewController::new(mtm);
        shows.setView(&picture);
        let preview = NSPopover::new(mtm);
        preview.setContentViewController(Some(&shows));
        // It comes and goes with the pointer, never taking the keys.
        preview.setBehavior(NSPopoverBehavior::ApplicationDefined);
        preview.setAnimates(false);
        let ivars = Ivars {
            preview,
            picture,
            previewed: RefCell::new(None),
            window: window.clone(),
            rows: rows.clone(),
            filters: filters.clone(),
            summary,
            reveal: reveal.clone(),
            act: act.clone(),
            filter: Cell::new(AttachmentFilter::All),
            files: RefCell::new(Vec::new()),
            missing: RefCell::new(Vec::new()),
            asks: RefCell::new(Some(asks)),
        };
        let controller: Retained<Controller> = unsafe { msg_send![super(Controller::alloc(mtm).set_ivars(ivars)), init] };
        // SAFETY: the controller is kept as long as the window and takes the actions.
        unsafe {
            for (control, action) in [(&**filters as &objc2_app_kit::NSControl, sel!(filterChanged:)), (&**reveal, sel!(revealChosen:)), (&**act, sel!(actOnChosen:))] {
                control.setTarget(Some(&controller));
                control.setAction(Some(action));
            }
        }
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_change.borrow_mut() = Some(Box::new(move || {
            if let Some(controller) = weak.load() {
                controller.buttons();
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_open.borrow_mut() = Some(Box::new(move |row| {
            if let Some(controller) = weak.load() {
                controller.open_row(row);
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_hover.borrow_mut() = Some(Box::new(move |row| {
            if let Some(controller) = weak.load() {
                controller.preview(row);
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_delete.borrow_mut() = Some(Box::new(move || {
            if let Some(controller) = weak.load().filter(|c| c.ivars().filter.get() != AttachmentFilter::Missing) {
                controller.delete_chosen();
            }
        }));
        Self { controller }
    }

    /// The window shown with the vault's attachments and its links to missing ones.
    pub fn show(&self, files: Vec<Attachment>, missing: Vec<MissingAttachment>) {
        self.set(files, missing);
        let window = &self.controller.ivars().window;
        if !window.isVisible() && !crate::probe::enabled() {
            window.center();
        }
        window.makeKeyAndOrderFront(None);
        window.makeFirstResponder(Some(&self.controller.ivars().rows));
    }

    /// The vault changed: its attachments and missing ones are these now.
    pub fn set(&self, files: Vec<Attachment>, missing: Vec<MissingAttachment>) {
        if *self.controller.ivars().files.borrow() == files && *self.controller.ivars().missing.borrow() == missing {
            return;
        }
        *self.controller.ivars().files.borrow_mut() = files;
        *self.controller.ivars().missing.borrow_mut() = missing;
        self.controller.list();
    }

    pub fn is_shown(&self) -> bool {
        self.controller.ivars().window.isVisible()
    }

    /// The filter's segments, the rows and the summary, for the probe:
    /// "Усі · 2 | Без посилань · 1 | Немає файлу · 1 || a.png — корінь бази · 7 Б · 1 посилання || 2 вкладення · 14 Б".
    pub fn describe(&self) -> String {
        let ivars = self.controller.ivars();
        let filters: Vec<String> = (0..3).map(|at| ivars.filters.labelForSegment(at).map(|label| label.to_string()).unwrap_or_default()).collect();
        let rows: Vec<String> = ivars.rows.ivars().items.borrow().iter().map(|item| { let (name, details) = said(item); format!("{name} — {details}") }).collect();
        format!("{} || {} || {}", filters.join(" | "), rows.join(" | "), ivars.summary.stringValue())
    }

    /// The pointer over the row `row`, and what its preview shows: "pic.png 120x80" (probe mode).
    pub fn probe_hover(&self, row: usize) -> String {
        self.controller.ivars().rows.hover(Some(row));
        self.controller.ivars().previewed.borrow().clone().unwrap_or_else(|| "none".to_owned())
    }

    /// The filter chosen, as a click on its segment (probe mode).
    pub fn probe_filter(&self, filter: AttachmentFilter) {
        self.controller.set_filter(filter);
    }

    /// The rows `rows` chosen (a click, then clicks with ⌘) and the right button clicked: files
    /// deleted, a missing one's note opened (probe mode).
    pub fn probe_act(&self, rows: &[usize]) {
        for (at, row) in rows.iter().enumerate() {
            self.controller.ivars().rows.pick(*row, if at == 0 { Pick::Row } else { Pick::Toggle });
        }
        // SAFETY: the button's action, as its click sends it.
        let _: () = unsafe { msg_send![&*self.controller, actOnChosen: std::ptr::null::<AnyObject>()] };
    }
}
