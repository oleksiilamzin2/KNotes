//! A deck's cards as a table, in a window of its own (ADR 0037; the owner asked for Anki's
//! browser): a row a card with its question, its deck and how it stands, sorted by a column
//! clicked; under the rows, the text of the card chosen, as its note has it, to be changed
//! there. The rows are drawn by the view itself, those in sight only, and a card's question
//! is read from its note when its row first shows: a deck of a hundred thousand cards opens
//! and scrolls as one of ten.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use knotes_app::{CardColumn, CardSearch, CardTable, Standing, card_of, in_deck, standing};
use knotes_cards::{Card, scan_cards};
use knotes_core::CardRow;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSBox, NSBoxType, NSButton, NSColor, NSEvent, NSFont, NSFontWeightSemibold, NSLayoutConstraint, NSResponder, NSScrollView, NSSearchField, NSStringDrawing, NSTextField, NSView,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::search_window::{attributes, fitted, rect};
use crate::strings;

/// A row's height, and the header's.
const ROW: f64 = 26.0;
const HEADER: f64 = 24.0;
const COLUMNS: [CardColumn; 6] = [CardColumn::Question, CardColumn::Deck, CardColumn::Due, CardColumn::Interval, CardColumn::Reviews, CardColumn::Lapses];
/// Notes whose cards are kept read: past that, all are let go (they are read again as their
/// rows show).
const NOTES_KEPT: usize = 200;

/// Where each column is in a table `width` wide (its left, its width): the question takes
/// what the others leave.
fn columns(width: f64) -> [(f64, f64); 6] {
    let fixed = [170.0, 110.0, 80.0, 80.0, 70.0];
    let question = (width - 24.0 - fixed.iter().sum::<f64>()).max(140.0);
    let mut places = [(0.0, 0.0); 6];
    let mut x = 12.0;
    for (place, wide) in places.iter_mut().zip(std::iter::once(question).chain(fixed)) {
        *place = (x, wide);
        x += wide;
    }
    places
}

type ReadNote = Box<dyn Fn(&str) -> Option<String>>;

/// The table's cards and what is read of their notes, shared by the views that draw them.
struct Model {
    table: RefCell<CardTable>,
    chosen: Cell<Option<usize>>,
    /// The rows chosen with it (⌘ or ⇧ held, ⌘A): what a row's menu does, it does to all.
    marked: RefCell<knotes_app::MarkedRows>,
    /// Each note read: its text and its cards.
    notes: RefCell<HashMap<String, Rc<(String, Vec<Card>)>>>,
    read: RefCell<Option<ReadNote>>,
}

impl Model {
    /// The note at `path`, read once while it is kept.
    fn note(&self, path: &str) -> Option<Rc<(String, Vec<Card>)>> {
        if let Some(read) = self.notes.borrow().get(path) {
            return Some(Rc::clone(read));
        }
        let text = self.read.borrow().as_ref()?(path)?;
        let cards = scan_cards(&text);
        let read = Rc::new((text, cards));
        let mut notes = self.notes.borrow_mut();
        if notes.len() >= NOTES_KEPT {
            notes.clear();
        }
        notes.insert(path.to_owned(), Rc::clone(&read));
        Some(read)
    }

    /// The card of `row` as its note has it now: its text there, and what it asks.
    fn card(&self, row: &CardRow) -> Option<(String, Card)> {
        let note = self.note(&row.path)?;
        let card = card_of(&note.1, row)?;
        Some((note.0.get(card.span.clone())?.to_owned(), card.clone()))
    }

    /// What the row at `at` says in each column.
    fn line(&self, at: usize, now: i64) -> Option<[String; 6]> {
        let table = self.table.borrow();
        let row = table.rows().get(at)?;
        // The question's first line: a row is one line.
        let question = self.card(row).map_or_else(|| "…".to_owned(), |(_, card)| plain(card.front.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default()));
        // When it comes: now, or in so long; a card never reviewed says so and no more.
        let when = |due: i64, wait: String| if due <= now { strings::CARD_NOW.to_owned() } else { strings::card_in(&wait) };
        let (due, interval) = match standing(row) {
            Standing::Paused => (strings::CARD_PAUSED.to_owned(), String::new()),
            Standing::New => (strings::CARD_NEW.to_owned(), String::new()),
            Standing::Learning(due) => (when(due, strings::review_wait(due - now)), strings::CARD_LEARNING.to_owned()),
            Standing::Review { due, days } => (when(due, strings::review_interval(((due - now) as f64 / 86_400.0).ceil())), strings::review_interval(days.round())),
        };
        let counted = |count: Option<u32>| count.map(|n| n.to_string()).unwrap_or_default();
        Some([question, row.deck.clone(), due, interval, counted(row.reviewed.map(|r| r.reviews)), counted(row.reviewed.map(|r| r.lapses))])
    }
}

/// `line` without the marks of its emphasis, its links and its code: a row says what a card
/// asks, not how it is written.
fn plain(line: &str) -> String {
    ["**", "__", "==", "[[", "]]", "`"].iter().fold(line.to_owned(), |text, mark| text.replace(mark, ""))
}

fn now() -> i64 {
    knotes_session::now()
}

type OnRow = Box<dyn Fn(usize)>;

/// How the rows chosen change: a row clicked (or come to by an arrow), clicked with ⌘, with ⇧,
/// ⇧ with an arrow (down, or up), ⌘A.
enum Pick {
    Row(usize),
    Toggle(usize),
    Extend(usize),
    Step(bool),
    All,
}

pub struct ListIvars {
    model: Rc<Model>,
    on_choose: RefCell<Option<Box<dyn Fn(Pick)>>>,
    on_open: RefCell<Option<OnRow>>,
    /// Called with the row a secondary click was on, and the click: its menu.
    on_menu: RefCell<Option<Box<dyn Fn(usize, &NSEvent)>>>,
}

define_class!(
    /// The cards, a row each, drawn by the view itself: those in sight only.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesCardRows"]
    #[ivars = ListIvars]
    pub struct CardList;

    impl CardList {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
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
            if at.y < 0.0 || row >= self.ivars().model.table.borrow().rows().len() {
                return;
            }
            if let Some(window) = self.window() {
                window.makeFirstResponder(Some(self));
            }
            if event.clickCount() >= 2 {
                if let Some(callback) = self.ivars().on_open.borrow().as_ref() {
                    callback(row);
                }
                return;
            }
            let held = event.modifierFlags();
            let pick = if held.contains(objc2_app_kit::NSEventModifierFlags::Command) {
                Pick::Toggle(row)
            } else if held.contains(objc2_app_kit::NSEventModifierFlags::Shift) {
                Pick::Extend(row)
            } else {
                Pick::Row(row)
            };
            if let Some(callback) = self.ivars().on_choose.borrow().as_ref() {
                callback(pick);
            }
        }

        /// ⌘A: every row.
        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_choose.borrow().as_ref() {
                callback(Pick::All);
            }
        }

        /// A secondary click on a row (two fingers, the right button): the row is chosen and
        /// its menu shows.
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let row = (at.y / ROW) as usize;
            if at.y < 0.0 || row >= self.ivars().model.table.borrow().rows().len() {
                return;
            }
            if let Some(callback) = self.ivars().on_menu.borrow().as_ref() {
                callback(row, event);
            }
        }

        /// Up and Down choose the row before and the next (with ⇧, that one too); Return opens
        /// the card's note.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if matches!(event.keyCode(), 125 | 126) && event.modifierFlags().contains(objc2_app_kit::NSEventModifierFlags::Shift) {
                if let Some(callback) = self.ivars().on_choose.borrow().as_ref() {
                    callback(Pick::Step(event.keyCode() == 125));
                }
                return;
            }
            let (chosen, rows) = (self.ivars().model.chosen.get(), self.ivars().model.table.borrow().rows().len());
            let to = match (event.keyCode(), chosen) {
                (125, Some(row)) if row + 1 < rows => Some(row + 1),
                (126, Some(row)) if row > 0 => Some(row - 1),
                (125 | 126, None) if rows > 0 => Some(0),
                (36, Some(row)) => {
                    if let Some(callback) = self.ivars().on_open.borrow().as_ref() {
                        callback(row);
                    }
                    return;
                }
                _ => None,
            };
            if let (Some(row), Some(callback)) = (to, self.ivars().on_choose.borrow().as_ref()) {
                callback(Pick::Row(row));
            }
        }
    }
);

impl CardList {
    fn new(mtm: MainThreadMarker, model: Rc<Model>) -> Retained<Self> {
        let ivars = ListIvars { model, on_choose: RefCell::new(None), on_open: RefCell::new(None), on_menu: RefCell::new(None) };
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(ivars)), init] }
    }

    /// As tall as its rows, as wide as `width`.
    fn fit(&self, width: f64) {
        self.setFrameSize(NSSize::new(width, self.ivars().model.table.borrow().rows().len() as f64 * ROW));
        self.setNeedsDisplay(true);
    }

    fn show_row(&self, row: usize) {
        self.scrollRectToVisible(rect(0.0, row as f64 * ROW, self.bounds().size.width, ROW));
        self.setNeedsDisplay(true);
    }

    fn draw(&self, dirty: NSRect) {
        let model = &self.ivars().model;
        let rows = model.table.borrow().rows().len();
        let width = self.bounds().size.width;
        let places = columns(width);
        let font = NSFont::systemFontOfSize(13.0);
        let small = NSFont::monospacedDigitSystemFontOfSize_weight(12.0, 0.0);
        let now = now();
        let first = (dirty.origin.y / ROW).max(0.0) as usize;
        let last = (((dirty.origin.y + dirty.size.height) / ROW) as usize + 1).min(rows);
        for row in first..last {
            let Some(line) = model.line(row, now) else { continue };
            let top = row as f64 * ROW;
            let chosen = model.chosen.get() == Some(row) || model.marked.borrow().holds(row);
            if chosen {
                NSColor::selectedContentBackgroundColor().setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(6.0, top + 1.0, width - 12.0, ROW - 2.0), 5.0, 5.0).fill();
            } else if row % 2 == 1 {
                NSColor::labelColor().colorWithAlphaComponent(0.035).setFill();
                NSBezierPath::fillRect(rect(0.0, top, width, ROW));
            }
            let (ink, quiet) = if chosen { (NSColor::whiteColor(), NSColor::whiteColor().colorWithAlphaComponent(0.85)) } else { (NSColor::labelColor(), NSColor::secondaryLabelColor()) };
            for (column, (text, (x, wide))) in line.iter().zip(places).enumerate() {
                let (font, colour) = if column == 0 { (&font, &ink) } else { (&small, &quiet) };
                // SAFETY: a font and a colour are those attributes' values.
                unsafe { NSString::from_str(&fitted(text, font, wide - 10.0)).drawAtPoint_withAttributes(NSPoint::new(x, top + 5.0), Some(&attributes(font, colour))) };
            }
        }
    }
}

type OnSort = Box<dyn Fn(CardColumn)>;

pub struct HeaderIvars {
    model: Rc<Model>,
    on_sort: RefCell<Option<OnSort>>,
}

define_class!(
    /// The columns' names over the rows: a click sorts by a column, again the other way.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesCardColumns"]
    #[ivars = HeaderIvars]
    pub struct CardHeader;

    impl CardHeader {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let sorted = self.ivars().model.table.borrow().sorted();
            let font = NSFont::systemFontOfSize_weight(11.0, unsafe { NSFontWeightSemibold });
            for ((column, title), (x, _)) in COLUMNS.iter().zip(strings::CARD_COLUMNS).zip(columns(self.bounds().size.width)) {
                let mark = match sorted {
                    Some((by, up)) if by == *column => if up { " ↑" } else { " ↓" },
                    _ => "",
                };
                // SAFETY: a font and a colour are those attributes' values.
                unsafe { NSString::from_str(&format!("{title}{mark}")).drawAtPoint_withAttributes(NSPoint::new(x, 5.0), Some(&attributes(&font, &NSColor::secondaryLabelColor()))) };
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let column = columns(self.bounds().size.width).iter().position(|(x, wide)| at.x >= *x && at.x < x + wide);
            if let (Some(column), Some(callback)) = (column, self.ivars().on_sort.borrow().as_ref()) {
                callback(COLUMNS[column]);
            }
        }
    }
);

impl CardHeader {
    fn new(mtm: MainThreadMarker, model: Rc<Model>) -> Retained<Self> {
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(HeaderIvars { model, on_sort: RefCell::new(None) })), init] }
    }
}

/// Called with a card, its text as the table showed it and as it is typed now: writes it in
/// the card's note. Why not, when it could not be written.
type OnSave = Box<dyn Fn(&CardRow, &str, &str) -> Result<(), String>>;
type OnOpen = Box<dyn Fn(&CardRow)>;
/// Called with what is typed in the search: the notes the vault's index finds it in.
type OnFind = Box<dyn Fn(&str) -> Vec<String>>;

/// What the table asks of the vault's window.
pub struct Asks {
    /// A note's text.
    pub read: Box<dyn Fn(&str) -> Option<String>>,
    /// A card's text changed (as the table showed it, as it is now): written in its note.
    pub save: OnSave,
    /// A card's note shown, at the card.
    pub open: OnOpen,
    /// The notes holding what is searched.
    pub find: OnFind,
    /// A card paused (true) or resumed: its id (given now if it had none) and how many bytes
    /// its text grew by for it.
    pub pause: Box<dyn Fn(&CardRow, bool) -> Result<(String, usize), String>>,
    /// The deck to put a card in, asked of the user; none when called off.
    pub deck: Box<dyn Fn(&CardRow) -> Option<String>>,
    /// Whether the user means to delete a card, or so many cards with it.
    pub delete: Box<dyn Fn(&CardRow, usize) -> bool>,
}

pub struct ControllerIvars {
    window: Retained<NSWindow>,
    /// Over the table: what is typed there keeps the cards holding it, or of a deck named so.
    field: Retained<NSSearchField>,
    on_find: RefCell<Option<OnFind>>,
    model: Rc<Model>,
    list: Retained<CardList>,
    list_scroll: Retained<NSScrollView>,
    header: Retained<CardHeader>,
    /// The text of the card chosen, as its note has it, to be changed: a note's own view under
    /// its live preview, so a card looks there as it does in its note.
    editor: Retained<crate::note_view::NoteTextView>,
    live: Retained<crate::live_preview::LivePreview>,
    /// Where the card chosen is written, over its text.
    place: Retained<NSTextField>,
    /// The card whose text the editor holds, and that text as it was read.
    editing: RefCell<Option<(CardRow, String)>>,
    /// The deck's name, for the window's title.
    deck: RefCell<String>,
    on_save: RefCell<Option<OnSave>>,
    on_open: RefCell<Option<OnOpen>>,
    /// A card paused or resumed, the deck asked for, a delete meant (`Asks`).
    asks: RefCell<Option<(Box<dyn Fn(&CardRow, bool) -> Result<(String, usize), String>>, Box<dyn Fn(&CardRow) -> Option<String>>, Box<dyn Fn(&CardRow, usize) -> bool>)>>,
}

define_class!(
    /// The window's buttons' target and its delegate.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesCardTable"]
    #[ivars = ControllerIvars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSWindowDelegate for Controller {
        /// What is typed in the card chosen is written as the window goes.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            self.save();
        }
    }

    impl Controller {
        #[unsafe(method(saveCard:))]
        fn save_card(&self, _sender: Option<&AnyObject>) {
            self.save();
        }

        #[unsafe(method(openNote:))]
        fn open_note(&self, _sender: Option<&AnyObject>) {
            self.open_chosen();
        }

        #[unsafe(method(searchCards:))]
        fn search_cards(&self, _sender: Option<&AnyObject>) {
            self.search();
        }

        /// The row's menu: the card chosen paused or resumed, put in another deck, deleted.
        #[unsafe(method(pauseCard:))]
        fn pause_card(&self, _sender: Option<&AnyObject>) {
            self.pause_chosen();
        }

        #[unsafe(method(moveCard:))]
        fn move_card(&self, _sender: Option<&AnyObject>) {
            self.move_chosen();
        }

        #[unsafe(method(deleteCard:))]
        fn delete_card(&self, _sender: Option<&AnyObject>) {
            self.delete_chosen();
        }
    }
);

impl Controller {
    /// What is typed in the card chosen written in its note, if it was changed. False when it
    /// could not be written: the text stays in the editor.
    fn save(&self) -> bool {
        let Some((row, was)) = self.ivars().editing.borrow().clone() else { return true };
        let typed = self.ivars().editor.string().to_string();
        if typed == was {
            return true;
        }
        let written = self.ivars().on_save.borrow().as_ref().map_or(Err(String::new()), |save| save(&row, &was, &typed));
        if written.is_err() {
            return false;
        }
        // Read again when its row shows: its question may be another now. The cards after it
        // in its note stand elsewhere by what was typed.
        self.ivars().model.notes.borrow_mut().remove(&row.path);
        self.ivars().model.table.borrow_mut().block_changed(&row.path, row.start, was.len(), typed.len());
        let row = CardRow { end: (i64::from(row.end) + typed.len() as i64 - was.len() as i64).max(0) as u32, ..row };
        *self.ivars().editing.borrow_mut() = Some((row, typed));
        self.ivars().list.setNeedsDisplay(true);
        true
    }

    /// The row at `row` chosen: what was typed in the card before is written first, then this
    /// card's text shows below.
    fn choose(&self, row: usize) {
        let ivars = self.ivars();
        if ivars.model.chosen.get() == Some(row) && ivars.editing.borrow().is_some() {
            return;
        }
        // Not written: the card it was typed in stays chosen, its text as typed.
        if !self.save() {
            return;
        }
        ivars.model.chosen.set(Some(row));
        ivars.list.show_row(row);
        self.show_chosen();
    }

    /// The text of the card chosen in the editor, and where it is written over it.
    fn show_chosen(&self) {
        let ivars = self.ivars();
        let row = ivars.model.chosen.get().and_then(|at| ivars.model.table.borrow().rows().get(at).cloned());
        let block = row.as_ref().and_then(|row| ivars.model.card(row)).map(|(block, _)| block);
        ivars.live.show(&ivars.editor, row.as_ref().map_or("", |row| row.path.as_str()), block.as_deref().unwrap_or_default());
        ivars.editor.setEditable(block.is_some());
        ivars.place.setStringValue(&NSString::from_str(&row.as_ref().map(|row| strings::card_place(crate::vault_window::document_name(&row.path), &row.deck)).unwrap_or_default()));
        *ivars.editing.borrow_mut() = row.zip(block);
    }

    /// The cards the field's text keeps shown: those whose own text holds every word typed
    /// (any case), and those of a deck named so. The vault's index says which notes hold the
    /// words, and only those are read: a hundred thousand cards are not read to find three.
    /// The card chosen stays so if it is among them; else the first is.
    fn search(&self) {
        // What is typed in a card is written first: its row may go from sight.
        if !self.save() {
            return;
        }
        let ivars = self.ivars();
        let query = ivars.field.stringValue().to_string();
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let notes = if words.is_empty() { Vec::new() } else { ivars.on_find.borrow().as_ref().map(|find| find(query.trim())).unwrap_or_default() };
        // The index finds a word in a note; which of the note's cards hold it is read there.
        let mut found = Vec::new();
        for path in notes {
            let Some(text) = ivars.model.read.borrow().as_ref().and_then(|read| read(&path)) else { continue };
            for card in scan_cards(&text) {
                let block = text.get(card.span.clone()).unwrap_or_default().to_lowercase();
                if words.iter().all(|word| block.contains(word)) {
                    found.push((path.clone(), card.span.start as u32, card.span.end as u32));
                }
            }
        }
        let chosen = ivars.editing.borrow().as_ref().map(|(row, _)| row.clone());
        ivars.model.table.borrow_mut().search(&CardSearch::new(&query, found));
        self.keep_chosen(chosen.as_ref());
        if ivars.model.chosen.get().is_none() {
            *ivars.editing.borrow_mut() = None;
            if ivars.model.table.borrow().rows().is_empty() { self.show_chosen() } else { self.choose(0) }
        }
    }

    /// The rows chosen changed by a click or a key.
    fn pick(&self, pick: Pick) {
        let ivars = self.ivars();
        let model = &ivars.model;
        let rows = model.table.borrow().rows().len();
        match (pick, model.chosen.get()) {
            (Pick::Row(row), _) | (Pick::Extend(row), None) => {
                model.marked.borrow_mut().clear();
                self.choose(row);
            }
            (Pick::Toggle(row), chosen) => {
                let shown = model.marked.borrow_mut().toggle(chosen, row);
                self.choose(shown);
            }
            (Pick::Extend(row), Some(chosen)) => {
                model.marked.borrow_mut().extend(chosen, row);
                ivars.list.show_row(row);
            }
            (Pick::Step(down), Some(chosen)) => {
                let to = model.marked.borrow_mut().step(chosen, down, rows);
                ivars.list.show_row(to);
            }
            (Pick::All, _) => model.marked.borrow_mut().all(rows),
            (Pick::Step(_), None) => {}
        }
        ivars.list.setNeedsDisplay(true);
    }

    /// The rows chosen, the last first (a card deleted moves those after it), what was typed
    /// in the card shown written first. None when that could not be written.
    fn targets(&self) -> Vec<usize> {
        if !self.save() {
            return Vec::new();
        }
        let model = &self.ivars().model;
        let mut rows = model.marked.borrow().with(model.chosen.get());
        rows.reverse();
        rows
    }

    /// The card of the row at `at` as the table has it now, and its text as its note has it.
    fn card_at(&self, at: usize) -> Option<(CardRow, String)> {
        let model = &self.ivars().model;
        let row = model.table.borrow().rows().get(at).cloned()?;
        let block = model.card(&row)?.0;
        Some((row, block))
    }

    /// The card shown below read again after its row's menu changed it (or others with it).
    fn show_acted(&self) {
        *self.ivars().editing.borrow_mut() = None;
        self.show_chosen();
        self.ivars().list.setNeedsDisplay(true);
    }

    /// A secondary click on `row`: it is chosen (among the rows chosen together, they stay
    /// so), and its menu shows; what the menu does, it does to every row chosen.
    fn show_menu(&self, row: usize, event: &NSEvent) {
        if !self.ivars().model.marked.borrow().holds(row) {
            self.pick(Pick::Row(row));
        }
        let cards: Vec<CardRow> = self.targets().into_iter().filter_map(|at| self.card_at(at)).map(|(card, _)| card).collect();
        if cards.is_empty() {
            return;
        }
        let mtm = self.mtm();
        let menu = objc2_app_kit::NSMenu::new(mtm);
        let pause = if cards.iter().all(|card| card.paused) { strings::CARD_RESUME } else { strings::CARD_PAUSE };
        for (title, action) in [(pause, sel!(pauseCard:)), (strings::CARD_MOVE, sel!(moveCard:)), (strings::CARD_DELETE, sel!(deleteCard:))] {
            // SAFETY: the controller outlives its menu and takes the action.
            let item = unsafe { objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(objc2_app_kit::NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(action), &NSString::new()) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            menu.addItem(&item);
        }
        objc2_app_kit::NSMenu::popUpContextMenu_withEvent_forView(&menu, event, &self.ivars().list);
    }

    /// The cards chosen paused, or resumed if every one is paused: said in the review log (and
    /// a card given an id in its note, if it had none), and in their rows at once.
    fn pause_chosen(&self) {
        let ivars = self.ivars();
        let cards: Vec<(CardRow, String)> = self.targets().into_iter().filter_map(|at| self.card_at(at)).collect();
        let paused = !cards.iter().all(|(row, _)| row.paused);
        // Each read again in its turn: one given an id moves those after it in its note.
        for at in self.targets() {
            let Some((row, block)) = self.card_at(at).filter(|(row, _)| row.paused != paused) else { continue };
            let done = ivars.asks.borrow().as_ref().map(|(pause, _, _)| pause(&row, paused));
            let Some(Ok((id, grew))) = done else { break };
            let mut table = ivars.model.table.borrow_mut();
            table.block_changed(&row.path, row.start, block.len(), block.len() + grew);
            table.set_paused(&row, &id, paused);
            ivars.model.notes.borrow_mut().remove(&row.path);
        }
        self.show_acted();
    }

    /// The cards chosen put in the deck the user names: their marks in their notes say it.
    fn move_chosen(&self) {
        let ivars = self.ivars();
        let targets = self.targets();
        // Asked once, the deck of the card shown offered.
        let Some(shown) = ivars.model.chosen.get().or(targets.first().copied()).and_then(|at| self.card_at(at)) else { return };
        let Some(deck) = ivars.asks.borrow().as_ref().and_then(|(_, deck, _)| deck(&shown.0)) else { return };
        for at in targets {
            let Some((row, block)) = self.card_at(at) else { continue };
            let Some(moved) = in_deck(&block, &deck).filter(|moved| *moved != block) else { continue };
            if ivars.on_save.borrow().as_ref().is_none_or(|save| save(&row, &block, &moved).is_err()) {
                break;
            }
            let mut table = ivars.model.table.borrow_mut();
            table.block_changed(&row.path, row.start, block.len(), moved.len());
            table.set_deck(&row, deck.trim().trim_matches('/'));
            ivars.model.notes.borrow_mut().remove(&row.path);
        }
        self.show_acted();
    }

    /// The cards chosen deleted from their notes, once the user says so: their text goes, the
    /// rest of the notes stays; the row after them is chosen.
    fn delete_chosen(&self) {
        let ivars = self.ivars();
        let targets = self.targets();
        let Some(shown) = ivars.model.chosen.get().or(targets.first().copied()).and_then(|at| self.card_at(at)) else { return };
        if !ivars.asks.borrow().as_ref().is_some_and(|(_, _, delete)| delete(&shown.0, targets.len())) {
            return;
        }
        let first = targets.last().copied().unwrap_or(0);
        for at in targets {
            let Some((row, block)) = self.card_at(at) else { continue };
            // With the block goes the gap it would leave (its line's end, a blank line of two):
            // what the note has there is taken with it, where the note is as the table read it.
            let (mut gone, mut block) = (row.clone(), block);
            let start = row.start as usize;
            if let Some(text) = ivars.model.note(&row.path).map(|note| note.0.clone()).filter(|text| text.get(start..).is_some_and(|rest| rest.starts_with(&block))) {
                let wider = knotes_app::block_to_delete(&text, start..start + block.len());
                gone.start = wider.start as u32;
                block = text[wider].to_owned();
            }
            if ivars.on_save.borrow().as_ref().is_none_or(|save| save(&gone, &block, "").is_err()) {
                break;
            }
            let mut table = ivars.model.table.borrow_mut();
            table.remove(&row);
            table.block_changed(&row.path, gone.start, block.len(), 0);
            ivars.model.notes.borrow_mut().remove(&row.path);
        }
        *ivars.editing.borrow_mut() = None;
        self.keep_chosen(None);
        let rows = ivars.model.table.borrow().rows().len();
        if rows == 0 { self.show_chosen() } else { self.choose(first.min(rows - 1)) }
    }

    fn open_chosen(&self) {
        if !self.save() {
            return;
        }
        let row = self.ivars().editing.borrow().as_ref().map(|(row, _)| row.clone());
        if let (Some(row), Some(open)) = (row, self.ivars().on_open.borrow().as_ref()) {
            open(&row);
        }
    }

    fn sort_by(&self, column: CardColumn) {
        let ivars = self.ivars();
        let chosen = ivars.model.chosen.get().and_then(|at| ivars.model.table.borrow().rows().get(at).cloned());
        ivars.model.table.borrow_mut().sort_by(column);
        self.keep_chosen(chosen.as_ref());
        ivars.header.setNeedsDisplay(true);
    }

    /// The rows drawn anew, the card `chosen` still the one chosen where it stands now.
    fn keep_chosen(&self, chosen: Option<&CardRow>) {
        let ivars = self.ivars();
        let at = chosen.and_then(|chosen| ivars.model.table.borrow().position(chosen));
        ivars.model.chosen.set(at);
        // The rows are put anew: those chosen together were so by their places.
        ivars.model.marked.borrow_mut().clear();
        ivars.list.fit(ivars.list_scroll.contentSize().width);
        if let Some(at) = at {
            ivars.list.show_row(at);
        }
        let (shown, count) = (ivars.model.table.borrow().rows().len(), ivars.model.table.borrow().count());
        ivars.window.setTitle(&NSString::from_str(&strings::cards_table_title(&ivars.deck.borrow(), shown, count)));
    }
}

/// The window of a deck's cards.
pub struct CardTableWindow {
    controller: Retained<Controller>,
}

impl CardTableWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable | NSWindowStyleMask::Resizable;
        // SAFETY: a window made and kept by Rust (`setReleasedWhenClosed(false)`).
        let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(0.0, 0.0, 920.0, 640.0), style, NSBackingStoreType::Buffered, false) };
        unsafe { window.setReleasedWhenClosed(false) };
        crate::probe::conceal(&window);
        let field = NSSearchField::new(mtm);
        field.setPlaceholderString(Some(&NSString::from_str(strings::CARD_SEARCH_HINT)));
        // After a short pause in the typing, not at each letter: a search reads the notes found.
        field.setSendsWholeSearchString(false);
        let model = Rc::new(Model { table: RefCell::new(CardTable::default()), chosen: Cell::new(None), marked: RefCell::default(), notes: RefCell::new(HashMap::new()), read: RefCell::new(None) });
        let header = CardHeader::new(mtm, Rc::clone(&model));
        let list = CardList::new(mtm, Rc::clone(&model));
        let list_scroll = NSScrollView::new(mtm);
        list_scroll.setHasVerticalScroller(true);
        list_scroll.setAutohidesScrollers(true);
        list_scroll.setDrawsBackground(false);
        list_scroll.setDocumentView(Some(&list));
        list.setAutoresizingMask(objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable);
        // A note's own view and its live preview, as the vault's window has them: a card shows
        // on its plate, its question strong, its marks muted, and is typed in as in its note.
        let (editor_scroll, editor) = crate::note_view::note_view(mtm);
        editor_scroll.setAutohidesScrollers(true);
        let live = crate::live_preview::LivePreview::new(mtm);
        editor.setDelegate(Some(ProtocolObject::from_ref(&*live)));
        live.attach(&editor);
        let clicks = live.clone();
        editor.on_click(move |view, index, command| clicks.click(view, index, command));
        let hovers = live.clone();
        editor.on_hover(move |index| hovers.clickable(index));
        let (look, shown) = (live.clone(), objc2::rc::Weak::from_retained(&editor));
        editor.on_appearance(move || {
            if let Some(shown) = shown.load() {
                look.appearance_changed(&shown);
            }
        });
        editor.setEditable(false);
        let place = NSTextField::labelWithString(&NSString::new(), mtm);
        place.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        place.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let button = |title: &str| {
            // SAFETY: no target yet (set below, once the controller is made).
            let button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm) };
            button.setControlSize(objc2_app_kit::NSControlSize::Small);
            button
        };
        let (open, save) = (button(strings::REVIEW_OPEN_NOTE), button(strings::CARD_SAVE));
        // ⌘S writes what is typed in the card, as it writes a note.
        save.setKeyEquivalent(&NSString::from_str("s"));
        save.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::Command);
        let lines = [NSBox::new(mtm), NSBox::new(mtm)];
        let content = NSView::new(mtm);
        for part in [&*field as &NSView, &header, &list_scroll, &lines[0], &lines[1], &place, &open, &save, &editor_scroll] {
            part.setTranslatesAutoresizingMaskIntoConstraints(false);
            content.addSubview(part);
        }
        for line in &lines {
            line.setBoxType(NSBoxType::Separator);
        }
        let constraints = [
            field.topAnchor().constraintEqualToAnchor_constant(&content.topAnchor(), 10.0),
            field.leadingAnchor().constraintEqualToAnchor_constant(&content.leadingAnchor(), 12.0),
            field.trailingAnchor().constraintEqualToAnchor_constant(&content.trailingAnchor(), -12.0),
            header.topAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), 6.0),
            header.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            header.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            header.heightAnchor().constraintEqualToConstant(HEADER),
            lines[0].topAnchor().constraintEqualToAnchor(&header.bottomAnchor()),
            lines[0].leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            lines[0].trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            list_scroll.topAnchor().constraintEqualToAnchor(&lines[0].bottomAnchor()),
            list_scroll.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            list_scroll.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            lines[1].topAnchor().constraintEqualToAnchor(&list_scroll.bottomAnchor()),
            lines[1].leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            lines[1].trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            // Under the rows: where the card is written and its buttons, then its text.
            place.topAnchor().constraintEqualToAnchor_constant(&lines[1].bottomAnchor(), 8.0),
            place.leadingAnchor().constraintEqualToAnchor_constant(&content.leadingAnchor(), 12.0),
            save.trailingAnchor().constraintEqualToAnchor_constant(&content.trailingAnchor(), -12.0),
            save.centerYAnchor().constraintEqualToAnchor(&place.centerYAnchor()),
            open.trailingAnchor().constraintEqualToAnchor_constant(&save.leadingAnchor(), -8.0),
            open.centerYAnchor().constraintEqualToAnchor(&place.centerYAnchor()),
            editor_scroll.topAnchor().constraintEqualToAnchor_constant(&place.bottomAnchor(), 6.0),
            editor_scroll.leadingAnchor().constraintEqualToAnchor(&content.leadingAnchor()),
            editor_scroll.trailingAnchor().constraintEqualToAnchor(&content.trailingAnchor()),
            editor_scroll.bottomAnchor().constraintEqualToAnchor(&content.bottomAnchor()),
            // The card's text has the lower third of the window.
            editor_scroll.heightAnchor().constraintEqualToAnchor_multiplier(&content.heightAnchor(), 0.3),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        window.setContentView(Some(&content));
        window.setContentSize(NSSize::new(920.0, 640.0));
        let ivars = ControllerIvars {
            window,
            field,
            on_find: RefCell::new(None),
            model,
            list,
            list_scroll,
            header,
            editor,
            live,
            place,
            editing: RefCell::new(None),
            deck: RefCell::new(String::new()),
            on_save: RefCell::new(None),
            on_open: RefCell::new(None),
            asks: RefCell::new(None),
        };
        // SAFETY: a plain object's init.
        let controller: Retained<Controller> = unsafe { msg_send![super(Controller::alloc(mtm).set_ivars(ivars)), init] };
        // SAFETY: the window keeps its controller as long as its buttons.
        unsafe {
            save.setTarget(Some(&controller));
            save.setAction(Some(sel!(saveCard:)));
            open.setTarget(Some(&controller));
            open.setAction(Some(sel!(openNote:)));
            controller.ivars().field.setTarget(Some(&controller));
            controller.ivars().field.setAction(Some(sel!(searchCards:)));
        }
        controller.ivars().window.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().list.ivars().on_choose.borrow_mut() = Some(Box::new(move |pick| {
            if let Some(controller) = weak.load() {
                controller.pick(pick);
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().list.ivars().on_open.borrow_mut() = Some(Box::new(move |row| {
            if let Some(controller) = weak.load() {
                controller.pick(Pick::Row(row));
                controller.open_chosen();
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().list.ivars().on_menu.borrow_mut() = Some(Box::new(move |row, event| {
            if let Some(controller) = weak.load() {
                controller.show_menu(row, event);
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *controller.ivars().header.ivars().on_sort.borrow_mut() = Some(Box::new(move |column| {
            if let Some(controller) = weak.load() {
                controller.sort_by(column);
            }
        }));
        Self { controller }
    }

    /// What the table asks of the vault's window (`Asks`).
    pub fn on(&self, asks: Asks) {
        let ivars = self.controller.ivars();
        *ivars.on_find.borrow_mut() = Some(asks.find);
        *ivars.model.read.borrow_mut() = Some(asks.read);
        *ivars.on_save.borrow_mut() = Some(asks.save);
        *ivars.on_open.borrow_mut() = Some(asks.open);
        *ivars.asks.borrow_mut() = Some((asks.pause, asks.deck, asks.delete));
    }

    /// Shows the window with the cards `rows` of the deck named `deck`, the first chosen.
    pub fn show(&self, deck: &str, rows: Vec<CardRow>) {
        let ivars = self.controller.ivars();
        // What was typed in a card of the deck shown before is written first.
        self.controller.save();
        *ivars.deck.borrow_mut() = deck.to_owned();
        *ivars.editing.borrow_mut() = None;
        // Another deck's cards: what was searched in the last is not searched in these.
        ivars.field.setStringValue(&NSString::new());
        ivars.model.notes.borrow_mut().clear();
        *ivars.model.table.borrow_mut() = CardTable::new(rows);
        self.controller.keep_chosen(None);
        ivars.header.setNeedsDisplay(true);
        if !ivars.model.table.borrow().rows().is_empty() {
            self.controller.choose(0);
        } else {
            self.controller.show_chosen();
        }
        if !crate::probe::enabled() {
            ivars.window.center();
        }
        ivars.window.makeKeyAndOrderFront(None);
        ivars.window.makeFirstResponder(Some(&ivars.list));
    }

    /// The window, while it shows (probe mode: its picture).
    pub fn window(&self) -> Option<Retained<NSWindow>> {
        let window = &self.controller.ivars().window;
        window.isVisible().then(|| window.clone())
    }

    /// Whether the window shows, and which deck's cards.
    pub fn shown(&self) -> Option<String> {
        self.controller.ivars().window.isVisible().then(|| self.controller.ivars().deck.borrow().clone())
    }

    /// The vault changed: the deck's cards as they are now, the card chosen still chosen.
    /// What is typed in it and not written stays as typed.
    pub fn reload(&self, rows: Vec<CardRow>) {
        let ivars = self.controller.ivars();
        let chosen = ivars.editing.borrow().as_ref().map(|(row, _)| row.clone());
        ivars.model.notes.borrow_mut().clear();
        ivars.model.table.borrow_mut().reload(rows);
        self.controller.keep_chosen(chosen.as_ref());
        // What is searched is searched in the cards as they are now.
        if !ivars.field.stringValue().to_string().trim().is_empty() {
            self.controller.search();
        }
        // Nothing typed since it was read: its text as its note has it now.
        let unchanged = ivars.editing.borrow().as_ref().is_none_or(|(_, was)| ivars.editor.string().to_string() == *was);
        if unchanged {
            self.controller.show_chosen();
        }
    }

    /// The table as drawn, for the probe: how many cards, how it is sorted, then each of the
    /// first rows (the one chosen in brackets), and the text below.
    pub fn describe(&self) -> String {
        let ivars = self.controller.ivars();
        let table = ivars.model.table.borrow();
        let sorted = table.sorted().map_or("as written".to_owned(), |(column, up)| format!("{column:?} {}", if up { "up" } else { "down" }));
        let count = table.rows().len();
        drop(table);
        let rows: Vec<String> = (0..count.min(20))
            .filter_map(|at| ivars.model.line(at, now()))
            .enumerate()
            .map(|(at, line)| {
                let said = line.iter().filter(|cell| !cell.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
                if ivars.model.chosen.get() == Some(at) {
                    format!("[{said}]")
                } else if ivars.model.marked.borrow().holds(at) {
                    format!("({said})")
                } else {
                    said
                }
            })
            .collect();
        format!("{} ({count} cards, {sorted}): {} ~ {}", ivars.window.title(), rows.join(" | "), ivars.editor.string().to_string().replace('\n', "⏎"))
    }

    /// `query` typed in the search (probe mode).
    pub fn probe_find(&self, query: &str) {
        self.controller.ivars().field.setStringValue(&NSString::from_str(query));
        self.controller.search();
    }

    /// The rows at `rows` chosen (the first clicked, the rest with ⌘) and an item of their
    /// menu: `pause` (or resume, when all are paused), `deck` (the deck asked for), `delete`;
    /// `choose` leaves them chosen (probe mode).
    pub fn probe_act(&self, rows: &[usize], act: &str) {
        for (i, &row) in rows.iter().enumerate() {
            self.controller.pick(if i == 0 { Pick::Row(row) } else { Pick::Toggle(row) });
        }
        match act {
            "pause" => self.controller.pause_chosen(),
            "deck" => self.controller.move_chosen(),
            "delete" => self.controller.delete_chosen(),
            _ => {}
        }
    }

    /// A click on a column's name (probe mode).
    pub fn probe_sort(&self, column: &str) {
        let column = match column {
            "deck" => CardColumn::Deck,
            "due" => CardColumn::Due,
            "interval" => CardColumn::Interval,
            "reviews" => CardColumn::Reviews,
            "lapses" => CardColumn::Lapses,
            _ => CardColumn::Question,
        };
        self.controller.sort_by(column);
    }

    /// The row at `row` chosen, `text` typed in the place of its card's, and ⌘S (probe mode).
    pub fn probe_edit(&self, row: usize, text: &str) -> bool {
        self.controller.choose(row);
        let ivars = self.controller.ivars();
        let path = ivars.editing.borrow().as_ref().map(|(row, _)| row.path.clone()).unwrap_or_default();
        ivars.live.show(&ivars.editor, &path, text);
        self.controller.save()
    }
}
