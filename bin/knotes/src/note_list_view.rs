//! Note list: a view-based `NSTableView` over `knotes_app::build_note_list` items. Only the
//! visible rows are asked for, so 50k notes cost the same as 50. Its context menu renames a
//! note in its row or moves it (ADR 0019).

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use knotes_app::{Clock, DateGroup, ListItem, TimeStyle, days_from_civil, time_style, title_of};
use knotes_core::NoteRow;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSControl, NSControlTextEditingDelegate, NSFont, NSLayoutConstraintOrientation, NSLineBreakMode, NSMenu, NSMenuItem,
    NSScrollView, NSTableCellView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSTextField,
    NSPasteboardItem, NSPasteboardWriting, NSTextFieldDelegate, NSTextView, NSUserInterfaceItemIdentification, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::quiet_scroll_view::QuietScrollView;
use crate::strings;

const NOTE_ROW_HEIGHT: CGFloat = 52.0;
const HEADER_ROW_HEIGHT: CGFloat = 28.0;

/// Date formatters following the macOS region settings.
struct Formatters {
    time: Retained<NSDateFormatter>,
    weekday: Retained<NSDateFormatter>,
    date: Retained<NSDateFormatter>,
    month: Retained<NSDateFormatter>,
}

impl Formatters {
    fn new() -> Self {
        let styled = |date: NSDateFormatterStyle, time: NSDateFormatterStyle| {
            let f = NSDateFormatter::new();
            f.setDateStyle(date);
            f.setTimeStyle(time);
            f
        };
        let template = |t: &str| {
            let f = NSDateFormatter::new();
            f.setLocalizedDateFormatFromTemplate(&NSString::from_str(t));
            f
        };
        Self {
            time: styled(NSDateFormatterStyle::NoStyle, NSDateFormatterStyle::ShortStyle),
            weekday: template("EEEE"),
            date: styled(NSDateFormatterStyle::ShortStyle, NSDateFormatterStyle::NoStyle),
            // Stand-alone month name ("березень", not "березня").
            month: template("LLLL"),
        }
    }
}

type OnSelect = Box<dyn Fn(String)>;
type OnRename = Box<dyn Fn(String, String)>;

pub struct Ivars {
    /// Called with the path of a note the user selected.
    on_select: RefCell<Option<OnSelect>>,
    /// Called with a note's path and the name typed in its row.
    on_rename: RefCell<Option<OnRename>>,
    table: RefCell<Weak<NSTableView>>,
    /// Called with the path of a note to move.
    on_move: RefCell<Option<OnSelect>>,
    /// Called with the path of a note to open in a new tab (ADR 0024).
    on_new_tab: RefCell<Option<OnSelect>>,
    /// The note whose row title is being edited, and that title field.
    renaming: RefCell<Option<(String, Retained<NSTextField>)>>,
    rows: RefCell<Arc<Vec<NoteRow>>>,
    items: RefCell<Vec<ListItem>>,
    clock: Cell<Clock>,
    formatters: Formatters,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesNoteListSource"]
    #[ivars = Ivars]
    pub struct NoteListSource;

    unsafe impl NSObjectProtocol for NoteListSource {}

    unsafe impl NSTableViewDataSource for NoteListSource {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table: &NSTableView) -> NSInteger {
            self.ivars().items.borrow().len() as NSInteger
        }

        /// A note row drags its path, for a sidebar folder to move it to (ADR 0019).
        #[unsafe(method_id(tableView:pasteboardWriterForRow:))]
        fn pasteboard_writer(&self, _table: &NSTableView, row: NSInteger) -> Option<Retained<ProtocolObject<dyn NSPasteboardWriting>>> {
            self.note_writer(row)
        }
    }

    unsafe impl NSControlTextEditingDelegate for NoteListSource {
        /// Enter (or a click elsewhere) renames the note.
        #[unsafe(method(controlTextDidEndEditing:))]
        fn text_did_end_editing(&self, _notification: &NSNotification) {
            self.finish_rename(true);
        }

        /// Escape leaves the name as it was.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, _control: &NSControl, _view: &NSTextView, command: Sel) -> bool {
            let cancel = command == sel!(cancelOperation:);
            if cancel {
                self.finish_rename(false);
            }
            cancel
        }
    }

    unsafe impl NSTextFieldDelegate for NoteListSource {}

    impl NoteListSource {
        #[unsafe(method(renameNote:))]
        fn rename_note(&self, _sender: Option<&AnyObject>) {
            if let Some(row) = self.menu_row() {
                self.begin_rename(row);
            }
        }

        #[unsafe(method(openInNewTab:))]
        fn open_in_new_tab(&self, _sender: Option<&AnyObject>) {
            let path = self.menu_row().and_then(|row| self.path_at(row));
            if let (Some(path), Some(callback)) = (path, self.ivars().on_new_tab.borrow().as_ref()) {
                callback(path);
            }
        }

        #[unsafe(method(moveNote:))]
        fn move_note(&self, _sender: Option<&AnyObject>) {
            let path = self.menu_row().and_then(|row| self.path_at(row));
            if let (Some(path), Some(callback)) = (path, self.ivars().on_move.borrow().as_ref()) {
                callback(path);
            }
        }

    }

    unsafe impl NSTableViewDelegate for NoteListSource {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_row(&self, table: &NSTableView, _column: Option<&NSTableColumn>, row: NSInteger) -> Option<Retained<NSView>> {
            self.cell_for(table, row as usize)
        }

        #[unsafe(method(tableView:isGroupRow:))]
        fn is_group_row(&self, _table: &NSTableView, row: NSInteger) -> bool {
            self.is_header(row)
        }

        #[unsafe(method(tableView:heightOfRow:))]
        fn height_of_row(&self, _table: &NSTableView, row: NSInteger) -> CGFloat {
            if self.is_header(row) { HEADER_ROW_HEIGHT } else { NOTE_ROW_HEIGHT }
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_did_change(&self, notification: &NSNotification) {
            let Some(table) = notification.object().and_then(|o| o.downcast::<NSTableView>().ok()) else { return };
            let path = (table.selectedRow() >= 0).then(|| self.path_at(table.selectedRow() as usize)).flatten();
            if let (Some(path), Some(callback)) = (path, self.ivars().on_select.borrow().as_ref()) {
                callback(path);
            }
        }

        #[unsafe(method(tableView:shouldSelectRow:))]
        fn should_select_row(&self, _table: &NSTableView, row: NSInteger) -> bool {
            !self.is_header(row)
        }
    }
);

impl NoteListSource {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = Ivars {
            on_select: RefCell::new(None),
            on_rename: RefCell::new(None),
            on_move: RefCell::new(None),
            on_new_tab: RefCell::new(None),
            table: RefCell::new(Weak::default()),
            renaming: RefCell::new(None),
            rows: RefCell::new(Arc::new(Vec::new())),
            items: RefCell::new(Vec::new()),
            clock: Cell::new(now()),
            formatters: Formatters::new(),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    pub fn set_items(&self, rows: Arc<Vec<NoteRow>>, items: Vec<ListItem>) {
        *self.ivars().rows.borrow_mut() = rows;
        *self.ivars().items.borrow_mut() = items;
        self.ivars().clock.set(now());
    }

    pub fn on_select(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_select.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_rename(&self, callback: impl Fn(String, String) + 'static) {
        *self.ivars().on_rename.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_new_tab(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_new_tab.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_move(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_move.borrow_mut() = Some(Box::new(callback));
    }


    /// The dragged form of the note in `row`: its vault path, under `NOTE_PATH_TYPE`.
    pub fn note_writer(&self, row: NSInteger) -> Option<Retained<ProtocolObject<dyn NSPasteboardWriting>>> {
        let path = usize::try_from(row).ok().and_then(|row| self.path_at(row))?;
        let item = NSPasteboardItem::new();
        item.setString_forType(&NSString::from_str(&path), &NSString::from_str(NOTE_PATH_TYPE));
        Some(ProtocolObject::from_retained(item))
    }

    /// The note row the context menu was opened on, else the selected one.
    fn menu_row(&self) -> Option<usize> {
        let table = self.ivars().table.borrow().load()?;
        let row = if table.clickedRow() >= 0 { table.clickedRow() } else { table.selectedRow() };
        (row >= 0 && !self.is_header(row)).then_some(row as usize)
    }

    /// Lets the title of the note in `row` be edited in place, all of it selected.
    pub fn begin_rename(&self, row: usize) {
        let Some(table) = self.ivars().table.borrow().load() else { return };
        let Some(path) = self.path_at(row) else { return };
        let cell = table.viewAtColumn_row_makeIfNecessary(0, row as NSInteger, true).and_then(|v| v.downcast::<NSTableCellView>().ok());
        let Some(title) = cell.and_then(|cell| field(&cell, TITLE_ID)) else { return };
        title.setEditable(true);
        title.setSelectable(true);
        // SAFETY: the source outlives the table's cells (it is the table's data source).
        unsafe { title.setDelegate(Some(ProtocolObject::from_ref(self))) };
        *self.ivars().renaming.borrow_mut() = Some((path, title.clone()));
        if let Some(window) = table.window() {
            window.makeFirstResponder(Some(&title));
        }
    }

    /// The title field being edited, for probe mode.
    pub fn renaming_field(&self) -> Option<Retained<NSTextField>> {
        self.ivars().renaming.borrow().as_ref().map(|(_, field)| field.clone())
    }

    /// Ends editing a row's title: renames the note when `commit` and the name changed,
    /// otherwise shows its name again.
    fn finish_rename(&self, commit: bool) {
        let Some((path, title)) = self.ivars().renaming.borrow_mut().take() else { return };
        let name = title.stringValue().to_string();
        title.setEditable(false);
        title.setSelectable(false);
        let old = title_of(&path).to_owned();
        let table = self.ivars().table.borrow().load();
        if let Some(window) = table.as_ref().and_then(|t| t.window()) {
            window.makeFirstResponder(table.as_deref().map(|t| t.as_ref()));
        }
        if !commit || name.trim().is_empty() || name == old {
            title.setStringValue(&NSString::from_str(&old));
            return;
        }
        if let Some(callback) = self.ivars().on_rename.borrow().as_ref() {
            callback(path, name);
        }
    }

    /// Path of the note shown in `row`; `None` for headers.
    pub fn path_at(&self, row: usize) -> Option<String> {
        let i = self.ivars().items.borrow().get(row)?.row()?;
        self.ivars().rows.borrow().get(i as usize).map(|r| r.path.clone())
    }

    /// Row showing the note at `path`, if it is in the list.
    pub fn row_of(&self, path: &str) -> Option<usize> {
        let rows = self.ivars().rows.borrow();
        self.ivars().items.borrow().iter().position(|item| item.row().is_some_and(|i| rows[i as usize].path == path))
    }

    fn is_header(&self, row: NSInteger) -> bool {
        matches!(self.ivars().items.borrow().get(row as usize), Some(ListItem::Header(_)))
    }

    /// Notes (not headers) in the list.
    pub fn note_count(&self) -> usize {
        self.ivars().items.borrow().iter().filter(|i| i.row().is_some()).count()
    }

    fn cell_for(&self, table: &NSTableView, index: usize) -> Option<Retained<NSView>> {
        let item = self.ivars().items.borrow().get(index)?.clone();
        let mtm = self.mtm();
        match item {
            ListItem::Header(group) => {
                let cell = header_cell(table, mtm);
                if let Some(field) = unsafe { cell.textField() } {
                    field.setStringValue(&NSString::from_str(&self.group_label(group)));
                }
                Some(Retained::into_super(cell))
            }
            ListItem::Note(i) | ListItem::Found(i, _) => {
                let rows = self.ivars().rows.borrow();
                let row = rows.get(i as usize)?;
                let cell = note_cell(table, mtm);
                set_field(&cell, TITLE_ID, title_of(&row.path));
                set_field(&cell, TIME_ID, &self.time_label(row.mtime_ns));
                // A search result shows the text around the match instead of the note's start.
                let snippet = match &item {
                    ListItem::Found(_, found) => found.as_str(),
                    _ => row.snippet.as_str(),
                };
                set_field(&cell, SNIPPET_ID, snippet);
                Some(Retained::into_super(cell))
            }
        }
    }

    fn time_label(&self, mtime_ns: i64) -> String {
        let f = &self.ivars().formatters;
        let date = NSDate::dateWithTimeIntervalSince1970(mtime_ns as f64 / 1e9);
        match time_style(mtime_ns, &self.ivars().clock.get()) {
            TimeStyle::Time => f.time.stringFromDate(&date).to_string(),
            TimeStyle::Yesterday => strings::YESTERDAY.to_owned(),
            TimeStyle::Weekday => f.weekday.stringFromDate(&date).to_string(),
            TimeStyle::Date => f.date.stringFromDate(&date).to_string(),
        }
    }

    fn group_label(&self, group: DateGroup) -> String {
        match group {
            DateGroup::Today => strings::TODAY.to_owned(),
            DateGroup::Yesterday => strings::YESTERDAY.to_owned(),
            DateGroup::Previous7Days => strings::PREVIOUS_7_DAYS.to_owned(),
            DateGroup::Previous30Days => strings::PREVIOUS_30_DAYS.to_owned(),
            DateGroup::Month { year, month } => {
                let mid = days_from_civil(year, month, 15) as f64 * 86_400.0;
                capitalize(&self.ivars().formatters.month.stringFromDate(&NSDate::dateWithTimeIntervalSince1970(mid)).to_string())
            }
            DateGroup::Year(year) => year.to_string(),
        }
    }
}

/// Current time and the local UTC offset from the system time zone.
pub fn now() -> Clock {
    let now_ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64);
    let utc_offset_s = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i32;
    Clock { now_ns, utc_offset_s }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The table inside a scroll view; `source` must outlive it (weak data source and delegate).
pub fn table(mtm: MainThreadMarker, source: &NoteListSource) -> (Retained<NSScrollView>, Retained<NSTableView>) {
    let table = NSTableView::new(mtm);
    table.setStyle(NSTableViewStyle::Inset);
    table.setHeaderView(None);
    table.setFloatsGroupRows(true);
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("note"));
    table.addTableColumn(&column);
    unsafe {
        table.setDataSource(Some(ProtocolObject::from_ref(source)));
        table.setDelegate(Some(ProtocolObject::from_ref(source)));
    }
    *source.ivars().table.borrow_mut() = Weak::new(&table);
    table.setDraggingSourceOperationMask_forLocal(objc2_app_kit::NSDragOperation::Move, true);
    let menu = NSMenu::new(mtm);
    for (title, action) in [(strings::OPEN_IN_NEW_TAB, sel!(openInNewTab:)), (strings::RENAME_NOTE, sel!(renameNote:)), (strings::MOVE_NOTE, sel!(moveNote:))] {
        // SAFETY: the source outlives the table (it is its data source), and implements the action.
        let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(action), ns_string!("")) };
        unsafe { item.setTarget(Some(source)) };
        menu.addItem(&item);
    }
    // SAFETY: a menu built here, kept by the table.
    unsafe { table.setMenu(Some(&menu)) };
    // Keeps the toolbar's glass without its separator line (ADR 0016).
    let scroll: Retained<NSScrollView> = Retained::into_super(QuietScrollView::new(mtm));
    scroll.setDocumentView(Some(&table));
    scroll.setHasVerticalScroller(true);
    (scroll, table)
}

/// Pasteboard type of a dragged note's vault path.
pub const NOTE_PATH_TYPE: &str = "app.knotes.note-path";

pub const TITLE_ID: &str = "title";
pub const TIME_ID: &str = "time";
pub const SNIPPET_ID: &str = "snippet";

fn header_cell(table: &NSTableView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    reuse(table, "HeaderCell").unwrap_or_else(|| {
        let cell = NSTableCellView::new(mtm);
        cell.setIdentifier(Some(ns_string!("HeaderCell")));
        let label = label(mtm, "", NSFont::boldSystemFontOfSize(NSFont::smallSystemFontSize()), &NSColor::secondaryLabelColor());
        cell.addSubview(&label);
        activate(&[
            label.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 8.0),
            label.bottomAnchor().constraintEqualToAnchor_constant(&cell.bottomAnchor(), -4.0),
        ]);
        unsafe { cell.setTextField(Some(&label)) };
        cell
    })
}

/// Bold title on the first line; time and snippet (grey) on the second, as in Knotes v1.
fn note_cell(table: &NSTableView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    reuse(table, "NoteCell").unwrap_or_else(|| {
        let cell = NSTableCellView::new(mtm);
        cell.setIdentifier(Some(ns_string!("NoteCell")));
        let title = label(mtm, TITLE_ID, NSFont::boldSystemFontOfSize(NSFont::systemFontSize()), &NSColor::labelColor());
        let time = label(mtm, TIME_ID, NSFont::systemFontOfSize(NSFont::systemFontSize()), &NSColor::secondaryLabelColor());
        let snippet = label(mtm, SNIPPET_ID, NSFont::systemFontOfSize(NSFont::systemFontSize()), &NSColor::secondaryLabelColor());
        // The snippet gives way first; the time is never cut.
        snippet.setContentCompressionResistancePriority_forOrientation(200.0, NSLayoutConstraintOrientation::Horizontal);
        time.setContentCompressionResistancePriority_forOrientation(800.0, NSLayoutConstraintOrientation::Horizontal);
        title.setContentCompressionResistancePriority_forOrientation(200.0, NSLayoutConstraintOrientation::Horizontal);
        for v in [&title, &time, &snippet] {
            cell.addSubview(v);
        }
        activate(&[
            title.topAnchor().constraintEqualToAnchor_constant(&cell.topAnchor(), 7.0),
            title.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 8.0),
            title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&cell.trailingAnchor(), -8.0),
            time.topAnchor().constraintEqualToAnchor_constant(&title.bottomAnchor(), 3.0),
            time.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            snippet.firstBaselineAnchor().constraintEqualToAnchor(&time.firstBaselineAnchor()),
            snippet.leadingAnchor().constraintEqualToAnchor_constant(&time.trailingAnchor(), 6.0),
            snippet.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&cell.trailingAnchor(), -8.0),
        ]);
        unsafe { cell.setTextField(Some(&title)) };
        cell
    })
}

fn reuse(table: &NSTableView, id: &str) -> Option<Retained<NSTableCellView>> {
    // SAFETY: views registered under these identifiers are always cells built in this module.
    unsafe { table.makeViewWithIdentifier_owner(&NSString::from_str(id), None) }.and_then(|v| v.downcast::<NSTableCellView>().ok())
}

fn label(mtm: MainThreadMarker, id: &str, font: Retained<NSFont>, color: &NSColor) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(ns_string!(""), mtm);
    field.setIdentifier(Some(&NSString::from_str(id)));
    field.setFont(Some(&font));
    field.setTextColor(Some(color));
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    field.setMaximumNumberOfLines(1);
    field.setTranslatesAutoresizingMaskIntoConstraints(false);
    field
}

fn activate(constraints: &[Retained<objc2_app_kit::NSLayoutConstraint>]) {
    for c in constraints {
        c.setActive(true);
    }
}

pub fn field(cell: &NSTableCellView, id: &str) -> Option<Retained<NSTextField>> {
    let id = NSString::from_str(id);
    cell.subviews()
        .iter()
        .find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id)))
        .and_then(|v| v.downcast::<NSTextField>().ok())
}

fn set_field(cell: &NSTableCellView, id: &str, text: &str) {
    if let Some(f) = field(cell, id) {
        f.setStringValue(&NSString::from_str(text));
    }
}
