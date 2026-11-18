//! The vault's search in a window of its own (the owner asked for it as DEVONthink's): the
//! toolbar's magnifying glass opens it. A field on top; under it every document of the vault
//! the words find, by its name first, then by its text; and in the lower half the document
//! chosen, shown as the vault's window shows a note (the live preview, read only), the words
//! marked, scrolled to the first. Return (or a double click) opens
//! the document in the vault's window; Escape closes the search.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use knotes_app::found_snippet;
use knotes_core::{ContentQuery, ContentSearch, NoteText, TitleSearch, Within};
use knotes_index::SqliteIndex;
use knotes_search::TitleIndex;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSBox, NSBoxType, NSColor, NSControl, NSControlTextEditingDelegate, NSEvent, NSFont, NSFontAttributeName, NSFontWeightSemibold, NSForegroundColorAttributeName,
    NSLayoutConstraint, NSResponder, NSScrollView, NSSearchField, NSSearchFieldDelegate, NSStringDrawing, NSTextFieldDelegate, NSTextView, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSDictionary, NSObject, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSStringCompareOptions};

use crate::strings;

/// Most documents one search lists: by name, and by text.
const MAX_NAMES: usize = 60;
const MAX_TEXTS: usize = 200;
/// A row of the results: a document's name with where it is, and the text found under it.
const ROW: f64 = 44.0;
const SNIPPET_CHARS: usize = 160;

/// A document found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    /// The text around what was found; empty for a document found by its name alone.
    pub snippet: String,
}

/// The vault's documents `query` finds: those whose name holds it, best first, then those whose
/// text holds every word (each once, with the text around the first find).
pub fn find(reader: &SqliteIndex, titles: Option<&TitleIndex>, query: &str) -> Vec<Found> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<Found> = titles.map(|t| t.search_titles(query, MAX_NAMES)).unwrap_or_default().into_iter().map(|hit| Found { path: hit.path, snippet: String::new() }).collect();
    let hits = reader.search(&ContentQuery { text: query, within: Within::Note, prefixes: true, scope: None, limit: MAX_TEXTS }).unwrap_or_default();
    for hit in hits {
        let text = reader.note_text(&hit.path).ok().flatten().unwrap_or_default();
        let block = text.get(hit.span.start as usize..hit.span.end as usize).unwrap_or(&text);
        let snippet = found_snippet(block, query, SNIPPET_CHARS);
        match found.iter_mut().find(|f| f.path == hit.path) {
            Some(by_name) if by_name.snippet.is_empty() => by_name.snippet = snippet,
            Some(_) => {}
            None => found.push(Found { path: hit.path, snippet }),
        }
    }
    found
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

fn attributes(font: &NSFont, colour: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &[font.as_ref() as &AnyObject, colour.as_ref()])
}

/// `text` cut with «…» to `room` points in `font`.
fn fitted(text: &str, font: &NSFont, room: f64) -> String {
    let width = |text: &str| unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes(font, &NSColor::labelColor()))) }.width;
    if width(text) <= room {
        return text.to_owned();
    }
    let letters: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0, letters.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let cut: String = letters[..middle].iter().chain(['…'].iter()).collect();
        if width(&cut) <= room { low = middle } else { high = middle - 1 }
    }
    letters[..low].iter().collect::<String>().trim_end().to_owned() + "…"
}

type OnRow = Box<dyn Fn(usize)>;

#[derive(Default)]
pub struct ListIvars {
    rows: RefCell<Vec<Found>>,
    chosen: Cell<Option<usize>>,
    on_choose: RefCell<Option<OnRow>>,
    on_open: RefCell<Option<OnRow>>,
}

define_class!(
    /// The documents found, a row each, drawn by the view itself.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSearchResults"]
    #[ivars = ListIvars]
    pub struct ResultList;

    impl ResultList {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty: NSRect) {
            self.draw(dirty);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let row = (at.y / ROW) as usize;
            if at.y < 0.0 || row >= self.ivars().rows.borrow().len() {
                return;
            }
            self.choose(row);
            let callback = if event.clickCount() >= 2 { &self.ivars().on_open } else { &self.ivars().on_choose };
            if let Some(callback) = callback.borrow().as_ref() {
                callback(row);
            }
        }
    }
);

impl ResultList {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(ListIvars::default())), init] }
    }

    /// Shows `rows`, the first chosen; as tall as they are, as wide as `width`.
    fn set(&self, rows: Vec<Found>, width: f64) {
        self.ivars().chosen.set((!rows.is_empty()).then_some(0));
        self.setFrameSize(NSSize::new(width, rows.len() as f64 * ROW));
        *self.ivars().rows.borrow_mut() = rows;
        self.setNeedsDisplay(true);
    }

    /// Row `row` chosen and scrolled into view.
    fn choose(&self, row: usize) {
        if row < self.ivars().rows.borrow().len() {
            self.ivars().chosen.set(Some(row));
            self.scrollRectToVisible(rect(0.0, row as f64 * ROW, self.bounds().size.width, ROW));
            self.setNeedsDisplay(true);
        }
    }

    fn draw(&self, dirty: NSRect) {
        let rows = self.ivars().rows.borrow();
        let width = self.bounds().size.width;
        let name_font = NSFont::systemFontOfSize_weight(13.0, unsafe { NSFontWeightSemibold });
        let small = NSFont::systemFontOfSize(11.0);
        let first = (dirty.origin.y / ROW).max(0.0) as usize;
        let last = (((dirty.origin.y + dirty.size.height) / ROW) as usize + 1).min(rows.len());
        for (row, found) in rows.iter().enumerate().take(last).skip(first) {
            let top = row as f64 * ROW;
            let chosen = self.ivars().chosen.get() == Some(row);
            if chosen {
                NSColor::selectedContentBackgroundColor().setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(6.0, top + 1.0, width - 12.0, ROW - 2.0), 6.0, 6.0).fill();
            }
            let (ink, quiet) = if chosen { (NSColor::whiteColor(), NSColor::whiteColor().colorWithAlphaComponent(0.8)) } else { (NSColor::labelColor(), NSColor::secondaryLabelColor()) };
            // The name at the left, the folder at the right; under them the text found.
            let (folder, name) = found.path.rsplit_once('/').unwrap_or(("", &found.path));
            let folder = fitted(folder, &small, width * 0.4);
            let folder_width = unsafe { NSString::from_str(&folder).sizeWithAttributes(Some(&attributes(&small, &quiet))) }.width;
            let name = fitted(knotes_app::title_of(name), &name_font, width - 36.0 - folder_width - 12.0);
            // SAFETY: a font and a colour are those attributes' values.
            unsafe {
                NSString::from_str(&name).drawAtPoint_withAttributes(NSPoint::new(18.0, top + 5.0), Some(&attributes(&name_font, &ink)));
                NSString::from_str(&folder).drawAtPoint_withAttributes(NSPoint::new(width - 18.0 - folder_width, top + 7.0), Some(&attributes(&small, &quiet)));
                NSString::from_str(&fitted(&found.snippet, &small, width - 36.0)).drawAtPoint_withAttributes(NSPoint::new(18.0, top + 24.0), Some(&attributes(&small, &quiet)));
            }
        }
    }
}

type OnOpen = Box<dyn Fn(String)>;

pub struct ControllerIvars {
    window: Retained<NSWindow>,
    field: Retained<NSSearchField>,
    list: Retained<ResultList>,
    list_scroll: Retained<NSScrollView>,
    /// The document chosen, as a note's view shows it: the live preview over a note's text
    /// view, not edited here.
    preview: Retained<crate::note_view::NoteTextView>,
    live: Retained<crate::live_preview::LivePreview>,
    reader: RefCell<Option<SqliteIndex>>,
    titles: RefCell<Option<Arc<TitleIndex>>>,
    on_open: RefCell<Option<OnOpen>>,
}

define_class!(
    /// What the search's field tells: its text changed, or a key that is the list's.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSearchWindowController"]
    #[ivars = ControllerIvars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSControlTextEditingDelegate for Controller {
        /// Down and up choose a document, Return opens it, Escape closes the search.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, _control: &NSControl, _view: &NSTextView, command: Sel) -> bool {
            self.key(command)
        }
    }

    unsafe impl NSTextFieldDelegate for Controller {}

    unsafe impl NSSearchFieldDelegate for Controller {}

    impl Controller {
        #[unsafe(method(queryChanged:))]
        fn query_changed(&self, _sender: Option<&AnyObject>) {
            self.search();
        }
    }
);

impl Controller {
    /// A key of the field that is the list's; false for one that is the field's own.
    fn key(&self, command: Sel) -> bool {
        if command == sel!(moveDown:) {
            self.step(1);
        } else if command == sel!(moveUp:) {
            self.step(-1);
        } else if command == sel!(insertNewline:) {
            self.open_chosen();
        } else if command == sel!(cancelOperation:) {
            self.ivars().window.orderOut(None);
        } else {
            return false;
        }
        true
    }

    /// The documents the field's text finds, listed; the first shown below.
    fn search(&self) {
        let query = self.ivars().field.stringValue().to_string();
        let found = match self.ivars().reader.borrow().as_ref() {
            Some(reader) => find(reader, self.ivars().titles.borrow().as_deref(), &query),
            None => Vec::new(),
        };
        self.ivars().list.set(found, self.ivars().list_scroll.contentSize().width);
        self.show_chosen();
    }

    fn chosen(&self) -> Option<Found> {
        let list = self.ivars().list.ivars();
        list.chosen.get().and_then(|row| list.rows.borrow().get(row).cloned())
    }

    /// The next row (`by` 1) or the one before (-1) chosen and shown below.
    fn step(&self, by: isize) {
        let list = &self.ivars().list;
        let (rows, chosen) = (list.ivars().rows.borrow().len(), list.ivars().chosen.get());
        if rows == 0 {
            return;
        }
        let next = chosen.map_or(0, |row| (row as isize + by).clamp(0, rows as isize - 1) as usize);
        list.choose(next);
        self.show_chosen();
    }

    /// The document chosen in the lower half, as a note is shown, the query's words marked,
    /// the first in view.
    fn show_chosen(&self) {
        let (preview, live) = (&self.ivars().preview, &self.ivars().live);
        let chosen = self.chosen();
        let text = chosen.as_ref().and_then(|found| self.ivars().reader.borrow().as_ref().and_then(|reader| reader.note_text(&found.path).ok().flatten())).unwrap_or_default();
        live.show(preview, chosen.as_ref().map_or("", |found| found.path.as_str()), &text);
        let query = self.ivars().field.stringValue().to_string();
        let marks: Vec<(usize, usize)> = marks(&preview.string(), &query).iter().map(|range| (range.location, range.location + range.length)).collect();
        live.show_found(preview, &marks);
        if marks.is_empty() {
            preview.scrollRangeToVisible(NSRange::new(0, 0));
        }
    }

    fn open_chosen(&self) {
        if let (Some(found), Some(callback)) = (self.chosen(), self.ivars().on_open.borrow().as_ref()) {
            self.ivars().window.orderOut(None);
            callback(found.path);
        }
    }
}

/// Where `text` holds a word of `query`, whatever the case, in the text's order.
fn marks(text: &NSString, query: &str) -> Vec<NSRange> {
    let mut found = Vec::new();
    for word in query.split_whitespace().filter(|w| w.chars().count() >= 2) {
        let word = NSString::from_str(word);
        let mut from = 0;
        while from < text.length() {
            let at = text.rangeOfString_options_range(&word, NSStringCompareOptions::CaseInsensitiveSearch, NSRange::new(from, text.length() - from));
            if at.length == 0 {
                break;
            }
            found.push(at);
            from = at.location + at.length;
        }
    }
    found.sort_by_key(|range| range.location);
    found
}

/// The vault's search window.
pub struct SearchWindow {
    controller: Retained<Controller>,
}

impl SearchWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable | NSWindowStyleMask::Resizable;
        // SAFETY: a window made and kept by Rust (`setReleasedWhenClosed(false)`).
        let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(0.0, 0.0, 860.0, 640.0), style, NSBackingStoreType::Buffered, false) };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(strings::SEARCH_VAULT));
        crate::probe::conceal(&window);
        let field = NSSearchField::new(mtm);
        field.setPlaceholderString(Some(&NSString::from_str(strings::SEARCH_VAULT_HINT)));
        field.setSendsSearchStringImmediately(true);
        field.setSendsWholeSearchString(false);
        let list = ResultList::new(mtm);
        let list_scroll = NSScrollView::new(mtm);
        list_scroll.setHasVerticalScroller(true);
        list_scroll.setAutohidesScrollers(true);
        list_scroll.setDrawsBackground(false);
        list_scroll.setDocumentView(Some(&list));
        // A note's own view and its live preview, as the vault's window has them; read only,
        // and its links lead nowhere from here.
        let (preview_scroll, preview) = crate::note_view::note_view(mtm);
        let live = crate::live_preview::LivePreview::new(mtm);
        preview.setDelegate(Some(ProtocolObject::from_ref(&*live)));
        live.attach(&preview);
        preview.setEditable(false);
        let (look, shown) = (live.clone(), objc2::rc::Weak::from_retained(&preview));
        preview.on_appearance(move || {
            if let Some(shown) = shown.load() {
                look.appearance_changed(&shown);
            }
        });
        let line = NSBox::new(mtm);
        line.setBoxType(NSBoxType::Separator);
        let content = NSView::new(mtm);
        for part in [&*field as &NSView, &list_scroll, &line, &preview_scroll] {
            part.setTranslatesAutoresizingMaskIntoConstraints(false);
            content.addSubview(part);
        }
        let constraints = [
            field.topAnchor().constraintEqualToAnchor_constant(&content.topAnchor(), 12.0),
            field.leadingAnchor().constraintEqualToAnchor_constant(&content.leadingAnchor(), 12.0),
            field.trailingAnchor().constraintEqualToAnchor_constant(&content.trailingAnchor(), -12.0),
            list_scroll.topAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), 10.0),
            list_scroll.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            list_scroll.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            line.topAnchor().constraintEqualToAnchor(&list_scroll.bottomAnchor()),
            line.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            line.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            preview_scroll.topAnchor().constraintEqualToAnchor(&line.bottomAnchor()),
            preview_scroll.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            preview_scroll.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            preview_scroll.bottomAnchor().constraintEqualToAnchor(&content.bottomAnchor()),
            // The document chosen has the lower half of the window (the owner's layout).
            preview_scroll.heightAnchor().constraintEqualToAnchor_multiplier(&content.heightAnchor(), 0.5),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        window.setContentView(Some(&content));
        window.setContentSize(NSSize::new(860.0, 640.0));
        let ivars = ControllerIvars { window, field, list, list_scroll, preview, live, reader: RefCell::new(None), titles: RefCell::new(None), on_open: RefCell::new(None) };
        // SAFETY: a plain object's init.
        let controller: Retained<Controller> = unsafe { msg_send![super(Controller::alloc(mtm).set_ivars(ivars)), init] };
        let field = &controller.ivars().field;
        // SAFETY: the window keeps its controller as long as its field.
        unsafe {
            field.setTarget(Some(&controller));
            field.setAction(Some(sel!(queryChanged:)));
            field.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
        }
        // A click on a row shows its document below; a double click opens it.
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().list.ivars().on_choose.borrow_mut() = Some(Box::new(move |_| {
            if let Some(controller) = weak.load() {
                controller.show_chosen();
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().list.ivars().on_open.borrow_mut() = Some(Box::new(move |_| {
            if let Some(controller) = weak.load() {
                controller.open_chosen();
            }
        }));
        Self { controller }
    }

    /// Called with the document opened from the search.
    pub fn on_open(&self, callback: impl Fn(String) + 'static) {
        *self.controller.ivars().on_open.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows the window over the vault at `vault` indexed at `index_dir`, its names as `titles`
    /// has them and its links as `links` resolves them, the field taking what is typed; what
    /// was searched before is searched again (the vault may have changed).
    pub fn show(&self, vault: &std::path::Path, index_dir: &std::path::Path, titles: Option<Arc<TitleIndex>>, links: Option<Arc<dyn knotes_core::LinkResolver>>) {
        let ivars = self.controller.ivars();
        // What the note's look needs of the vault: its images, and which links lead somewhere.
        ivars.live.set_vault(vault.to_path_buf());
        if let Some(links) = links {
            ivars.live.set_links(&ivars.preview, links);
        }
        *ivars.reader.borrow_mut() = SqliteIndex::open_reader(index_dir).ok();
        *ivars.titles.borrow_mut() = titles;
        ivars.window.center();
        ivars.window.makeKeyAndOrderFront(None);
        ivars.window.makeFirstResponder(Some(&ivars.field));
        self.controller.search();
    }

    /// The window itself, when it shows (probe mode: its picture, and whether it is open).
    pub fn window(&self) -> Option<Retained<NSWindow>> {
        let window = &self.controller.ivars().window;
        window.isVisible().then(|| window.clone())
    }

    /// `query` typed in the field (probe mode).
    pub fn type_query(&self, query: &str) {
        self.controller.ivars().field.setStringValue(&NSString::from_str(query));
        self.controller.search();
    }

    /// Down or up pressed, `steps` times (probe mode).
    pub fn step(&self, steps: isize) {
        for _ in 0..steps.abs() {
            self.controller.step(steps.signum());
        }
    }

    /// Return pressed (probe mode).
    pub fn open_chosen(&self) {
        self.controller.open_chosen();
    }

    /// The documents listed, the one chosen in brackets, and what the lower half shows of it:
    /// how many marks, and the text of the first (probe mode).
    pub fn describe(&self) -> String {
        let ivars = self.controller.ivars();
        let chosen = ivars.list.ivars().chosen.get();
        let rows: Vec<String> = ivars.list.ivars().rows.borrow().iter().enumerate().map(|(i, f)| if chosen == Some(i) { format!("[{}]", f.path) } else { f.path.clone() }).collect();
        let text = ivars.preview.string();
        let query = ivars.field.stringValue().to_string();
        let marks = marks(&text, &query);
        let first = marks.first().map(|range| text.substringWithRange(*range).to_string()).unwrap_or_default();
        format!("{} ~ {} marks, first «{first}», {} letters shown", rows.join(" | "), marks.len(), text.length())
    }
}
