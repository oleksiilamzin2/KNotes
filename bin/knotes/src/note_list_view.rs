//! Note list: a view-based `NSTableView` over `knotes_app::build_note_list` items. Only the
//! visible rows are asked for, so 50k notes cost the same as 50.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use knotes_app::{Clock, DateGroup, ListItem, TimeStyle, days_from_civil, time_style, title_of};
use knotes_core::NoteRow;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSColor, NSControlTextEditingDelegate, NSFont, NSLayoutConstraintOrientation, NSLineBreakMode, NSScrollView,
    NSTableCellView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle,
    NSTextField, NSUserInterfaceItemIdentification, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, ns_string};

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

pub struct Ivars {
    /// Called with the path of a note the user selected.
    on_select: RefCell<Option<OnSelect>>,
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
    }

    unsafe impl NSControlTextEditingDelegate for NoteListSource {}

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

    /// Path of the note shown in `row`; `None` for headers.
    pub fn path_at(&self, row: usize) -> Option<String> {
        let i = self.ivars().items.borrow().get(row)?.row()?;
        self.ivars().rows.borrow().get(i as usize).map(|r| r.path.clone())
    }

    /// Paths of the notes in the list, in list order.
    pub fn paths(&self) -> Vec<String> {
        let rows = self.ivars().rows.borrow();
        self.ivars().items.borrow().iter().filter_map(|i| i.row()).map(|i| rows[i as usize].path.clone()).collect()
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
    let scroll = NSScrollView::new(mtm);
    scroll.setDocumentView(Some(&table));
    scroll.setHasVerticalScroller(true);
    (scroll, table)
}

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
