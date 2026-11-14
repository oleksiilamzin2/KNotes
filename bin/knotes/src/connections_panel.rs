//! The connections of the document open, in the right panel (ADR 0026): notes linking to it
//! with the lines they do it on, notes naming it without a link, and its own links.

use std::cell::RefCell;
use std::ops::Range;

use knotes_app::{Mention, Outgoing};
use objc2::rc::Retained;
use objc2::{AnyThread, Message};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSColor, NSControlTextEditingDelegate, NSFont, NSImage, NSLineBreakMode, NSOutlineView, NSOutlineViewDataSource, NSOutlineViewDelegate, NSScrollView, NSTableCellView, NSTableColumn,
    NSTableViewStyle, NSTextField, NSUserInterfaceItemIdentification, NSView,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSInteger, NSMutableAttributedString, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::strings;

/// What the panel lists for a document.
#[derive(Debug, Default)]
pub struct Connections {
    /// Notes linking to it, each with its links (the lines around them).
    pub linked: Vec<(String, Vec<Mention>)>,
    /// Notes naming it without a link.
    pub unlinked: Vec<(String, Vec<Mention>)>,
    /// Its own links, by target.
    pub outgoing: Vec<Outgoing>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Linked,
    Unlinked,
    Outgoing,
}

/// A row of the panel.
#[derive(Debug, Clone)]
enum Row {
    Section(Section, usize),
    Note { path: String, count: usize },
    Line { path: String, mention: Mention, section: Section },
    Out { label: String, resolved: Option<String> },
}

type OnOpen = Box<dyn Fn(String, Option<Range<usize>>)>;
type OnFollow = Box<dyn Fn(String, Option<String>)>;
type OnMakeLink = Box<dyn Fn(String, Range<usize>)>;

pub struct SourceIvars {
    rows: RefCell<Vec<Row>>,
    children: RefCell<Vec<Vec<usize>>>,
    roots: RefCell<Vec<usize>>,
    objects: RefCell<Vec<Retained<NSNumber>>>,
    /// A line or note clicked: its note opened (at the line's link).
    on_open: RefCell<Option<OnOpen>>,
    /// An outgoing link clicked: its target (as written) and the file it resolves to.
    on_follow: RefCell<Option<OnFollow>>,
    /// An unlinked mention's button: made a link in its note.
    on_make_link: RefCell<Option<OnMakeLink>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesConnectionsSource"]
    #[ivars = SourceIvars]
    pub struct ConnectionsSource;

    unsafe impl NSObjectProtocol for ConnectionsSource {}

    unsafe impl NSControlTextEditingDelegate for ConnectionsSource {}

    unsafe impl NSOutlineViewDataSource for ConnectionsSource {
        #[unsafe(method(outlineView:numberOfChildrenOfItem:))]
        fn number_of_children(&self, _outline: &NSOutlineView, item: Option<&AnyObject>) -> NSInteger {
            self.children_of(item.and_then(index_of)).len() as NSInteger
        }

        #[unsafe(method_id(outlineView:child:ofItem:))]
        fn child(&self, _outline: &NSOutlineView, index: NSInteger, item: Option<&AnyObject>) -> Retained<AnyObject> {
            let child = self.children_of(item.and_then(index_of)).get(index as usize).copied().unwrap_or(0);
            self.object(child)
        }

        #[unsafe(method(outlineView:isItemExpandable:))]
        fn is_expandable(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            index_of(item).is_some_and(|i| !self.children_of(Some(i)).is_empty())
        }
    }

    unsafe impl NSOutlineViewDelegate for ConnectionsSource {
        #[unsafe(method_id(outlineView:viewForTableColumn:item:))]
        fn view_for_item(&self, outline: &NSOutlineView, _column: Option<&NSTableColumn>, item: &AnyObject) -> Option<Retained<NSView>> {
            let row = index_of(item).and_then(|i| Some((i, self.ivars().rows.borrow().get(i).cloned()?)));
            row.map(|(i, row)| Retained::into_super(self.cell_for(outline, i, &row)))
        }
    }

    impl ConnectionsSource {
        /// A click on a row.
        #[unsafe(method(rowClicked:))]
        fn row_clicked(&self, sender: &NSOutlineView) {
            // The row clicked; the one selected when the action comes from the keyboard or a probe.
            let row = if sender.clickedRow() >= 0 { sender.clickedRow() } else { sender.selectedRow() };
            let item = (row >= 0).then(|| sender.itemAtRow(row)).flatten().and_then(|o| index_of(&o));
            let Some(row) = item.and_then(|i| self.ivars().rows.borrow().get(i).cloned()) else { return };
            match row {
                Row::Note { path, .. } => self.open(path, None),
                Row::Line { path, mention, .. } => self.open(path, Some(mention.range)),
                Row::Out { label, resolved } => {
                    if let Some(callback) = self.ivars().on_follow.borrow().as_ref() {
                        callback(label, resolved);
                    }
                }
                Row::Section(..) => {}
            }
        }

        /// An unlinked mention's button.
        #[unsafe(method(makeLink:))]
        fn make_link(&self, sender: &NSButton) {
            let row = usize::try_from(sender.tag()).ok().and_then(|i| self.ivars().rows.borrow().get(i).cloned());
            if let (Some(Row::Line { path, mention, .. }), Some(callback)) = (row, self.ivars().on_make_link.borrow().as_ref()) {
                callback(path, mention.range);
            }
        }
    }
);

impl ConnectionsSource {
    fn children_of(&self, of: Option<usize>) -> Vec<usize> {
        match of {
            Some(i) => self.ivars().children.borrow().get(i).cloned().unwrap_or_default(),
            None => self.ivars().roots.borrow().clone(),
        }
    }

    fn object(&self, i: usize) -> Retained<AnyObject> {
        let object = self.ivars().objects.borrow().get(i).cloned().unwrap_or_else(|| NSNumber::new_usize(i));
        Retained::into_super(Retained::into_super(Retained::into_super(object)))
    }

    fn open(&self, path: String, range: Option<Range<usize>>) {
        if let Some(callback) = self.ivars().on_open.borrow().as_ref() {
            callback(path, range);
        }
    }

    fn cell_for(&self, outline: &NSOutlineView, index: usize, row: &Row) -> Retained<NSTableCellView> {
        let cell = cell(outline, self.mtm());
        // SAFETY: the cells are built by `cell` with their text field set.
        let Some(field) = (unsafe { cell.textField() }) else { return cell };
        let button = link_button(&cell);
        if let Some(button) = &button {
            button.setHidden(true);
        }
        field.setTextColor(Some(&NSColor::labelColor()));
        field.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
        field.setMaximumNumberOfLines(1);
        match row {
            Row::Section(section, count) => {
                field.setStringValue(&NSString::from_str(&section_title(*section, *count)));
                field.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::smallSystemFontSize())));
                field.setTextColor(Some(&NSColor::secondaryLabelColor()));
            }
            Row::Note { path, count } => field.setStringValue(&NSString::from_str(&format!("{} ({count})", document_name(path)))),
            Row::Line { mention, section, .. } => {
                field.setMaximumNumberOfLines(3);
                field.setAttributedStringValue(&line_text(mention));
                if let (Section::Unlinked, Some(button)) = (section, &button) {
                    button.setHidden(false);
                    button.setTag(index as NSInteger);
                }
            }
            Row::Out { label, resolved } => {
                field.setStringValue(&NSString::from_str(label));
                if resolved.is_none() {
                    field.setTextColor(Some(&NSColor::tertiaryLabelColor()));
                }
            }
        }
        if let Some(button) = &button {
            // SAFETY: the source outlives the outline's cells (it is the outline's data source).
            unsafe {
                button.setTarget(Some(self));
                button.setAction(Some(sel!(makeLink:)));
            }
        }
        cell
    }
}

/// The heading of a section and how many it holds.
fn section_title(section: Section, count: usize) -> String {
    let title = match section {
        Section::Linked => strings::LINKS_HERE,
        Section::Unlinked => strings::UNLINKED_MENTIONS,
        Section::Outgoing => strings::OUTGOING_LINKS,
    };
    format!("{title} · {count}")
}

/// A mention's line, the mention itself bold.
fn line_text(mention: &Mention) -> Retained<NSAttributedString> {
    let text = NSMutableAttributedString::new();
    let plain = |s: &str, font: &NSFont, colour: &NSColor| {
        // SAFETY: font and colour values of their attributes.
        let attrs = unsafe {
            NSDictionary::from_retained_objects(
                &[objc2_app_kit::NSFontAttributeName, objc2_app_kit::NSForegroundColorAttributeName],
                &[Retained::into_super(Retained::into_super(font.retain())), Retained::into_super(Retained::into_super(colour.retain()))],
            )
        };
        // SAFETY: a string and attributes of its own.
        unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(s), Some(&attrs)) }
    };
    let size = NSFont::smallSystemFontSize() + 1.0;
    let (body, bold) = (NSFont::systemFontOfSize(size), NSFont::boldSystemFontOfSize(size));
    let line = &mention.line;
    text.appendAttributedString(&plain(&line[..mention.mark.start], &body, &NSColor::secondaryLabelColor()));
    text.appendAttributedString(&plain(&line[mention.mark.clone()], &bold, &NSColor::labelColor()));
    text.appendAttributedString(&plain(&line[mention.mark.end..], &body, &NSColor::secondaryLabelColor()));
    Retained::into_super(text)
}

/// The name a document shows: without its folder, `.md` or `.pdf`.
fn document_name(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    [".md", ".pdf"].iter().find_map(|ext| name.len().checked_sub(ext.len()).filter(|&at| name[at..].eq_ignore_ascii_case(ext)).map(|at| &name[..at])).unwrap_or(name)
}

/// The row an item object stands for.
fn index_of(object: &AnyObject) -> Option<usize> {
    Some(object.downcast_ref::<NSNumber>()?.as_usize())
}

pub struct ConnectionsPanel {
    pub view: Retained<NSView>,
    outline: Retained<NSOutlineView>,
    /// Kept alive here: the outline view holds its data source weakly.
    source: Retained<ConnectionsSource>,
}

impl ConnectionsPanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let source = ConnectionsSource::alloc(mtm).set_ivars(SourceIvars {
            rows: RefCell::new(Vec::new()),
            children: RefCell::new(Vec::new()),
            roots: RefCell::new(Vec::new()),
            objects: RefCell::new(Vec::new()),
            on_open: RefCell::new(None),
            on_follow: RefCell::new(None),
            on_make_link: RefCell::new(None),
        });
        let source: Retained<ConnectionsSource> = unsafe { msg_send![super(source), init] };
        let outline = NSOutlineView::new(mtm);
        outline.setStyle(NSTableViewStyle::Plain);
        outline.setHeaderView(None);
        outline.setUsesAutomaticRowHeights(true);
        outline.setBackgroundColor(&NSColor::clearColor());
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("connection"));
        // As wide as the panel: the lines wrap in it, the buttons stay in view.
        column.setResizingMask(objc2_app_kit::NSTableColumnResizingOptions::AutoresizingMask);
        outline.setColumnAutoresizingStyle(objc2_app_kit::NSTableViewColumnAutoresizingStyle::FirstColumnOnlyAutoresizingStyle);
        outline.addTableColumn(&column);
        // SAFETY: the source outlives the outline (the panel keeps both) and takes the action.
        unsafe {
            outline.setOutlineTableColumn(Some(&column));
            outline.setDataSource(Some(ProtocolObject::from_ref(&*source)));
            outline.setDelegate(Some(ProtocolObject::from_ref(&*source)));
            outline.setTarget(Some(&source));
            outline.setAction(Some(sel!(rowClicked:)));
        }
        let scroll = NSScrollView::new(mtm);
        scroll.setDocumentView(Some(&outline));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let view = NSView::new(mtm);
        scroll.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&scroll);
        let guide = view.safeAreaLayoutGuide();
        for c in [
            scroll.topAnchor().constraintEqualToAnchor(&guide.topAnchor()),
            scroll.bottomAnchor().constraintEqualToAnchor(&view.bottomAnchor()),
            scroll.leadingAnchor().constraintEqualToAnchor(&view.leadingAnchor()),
            scroll.trailingAnchor().constraintEqualToAnchor(&view.trailingAnchor()),
        ] {
            c.setActive(true);
        }
        Self { view, outline, source }
    }

    /// Lists `connections`, every section and note unfolded.
    pub fn show(&self, connections: Connections) {
        let mut rows = Vec::new();
        let mut children: Vec<Vec<usize>> = Vec::new();
        let mut roots = Vec::new();
        let mut add = |row: Row, parent: Option<usize>, rows: &mut Vec<Row>, children: &mut Vec<Vec<usize>>| {
            rows.push(row);
            children.push(Vec::new());
            let i = rows.len() - 1;
            match parent {
                Some(p) => children[p].push(i),
                None => roots.push(i),
            }
            i
        };
        for (section, notes) in [(Section::Linked, &connections.linked), (Section::Unlinked, &connections.unlinked)] {
            let count = notes.iter().map(|(_, m)| m.len()).sum();
            let head = add(Row::Section(section, count), None, &mut rows, &mut children);
            for (path, mentions) in notes {
                let note = add(Row::Note { path: path.clone(), count: mentions.len() }, Some(head), &mut rows, &mut children);
                for mention in mentions {
                    add(Row::Line { path: path.clone(), mention: mention.clone(), section }, Some(note), &mut rows, &mut children);
                }
            }
        }
        let head = add(Row::Section(Section::Outgoing, connections.outgoing.len()), None, &mut rows, &mut children);
        for out in &connections.outgoing {
            add(Row::Out { label: out.label.clone(), resolved: out.resolved.clone() }, Some(head), &mut rows, &mut children);
        }
        let ivars = self.source.ivars();
        *ivars.objects.borrow_mut() = (0..rows.len()).map(NSNumber::new_usize).collect();
        *ivars.rows.borrow_mut() = rows;
        *ivars.children.borrow_mut() = children;
        *ivars.roots.borrow_mut() = roots;
        self.outline.reloadData();
        // SAFETY: no item: every one, unfolded.
        unsafe { self.outline.expandItem_expandChildren(None, true) };
    }

    pub fn on_open(&self, callback: impl Fn(String, Option<Range<usize>>) + 'static) {
        *self.source.ivars().on_open.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_follow(&self, callback: impl Fn(String, Option<String>) + 'static) {
        *self.source.ivars().on_follow.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_make_link(&self, callback: impl Fn(String, Range<usize>) + 'static) {
        *self.source.ivars().on_make_link.borrow_mut() = Some(Box::new(callback));
    }

    /// Each row as shown: `[section · n]`, `note (n)`, `·line` with the mention in «», ` ↗` on
    /// a mention that can be made a link, `target` or `target (немає)` (probe mode).
    pub fn describe(&self) -> String {
        let rows = self.source.ivars().rows.borrow();
        (0..self.outline.numberOfRows())
            .filter_map(|r| {
                let i = index_of(&*self.outline.itemAtRow(r)?)?;
                Some(match rows.get(i)? {
                    Row::Section(section, count) => format!("[{}]", section_title(*section, *count)),
                    Row::Note { path, count } => format!("{} ({count})", document_name(path)),
                    Row::Line { mention, section, .. } => {
                        let l = &mention.line;
                        let arrow = if *section == Section::Unlinked { " ↗" } else { "" };
                        format!("·{}«{}»{}{arrow}", &l[..mention.mark.start], &l[mention.mark.clone()], &l[mention.mark.end..])
                    }
                    Row::Out { label, resolved } => if resolved.is_some() { label.clone() } else { format!("{label} ({})", strings::NOT_THERE) },
                })
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// A click on the row described as `described` (probe mode): selected, the outline view's
    /// action sent to its target, as AppKit does after a click.
    pub fn click(&self, described: &str) {
        let shown = self.describe();
        let Some(row) = shown.split(" | ").position(|r| r.trim_start_matches('·') == described || r == described) else { return };
        let (outline, row) = (&self.outline, row as isize);
        let Some(action) = outline.action() else { return };
        outline.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row as usize), false);
        let target = outline.target();
        // SAFETY: the outline view's own action and target, with it as the sender.
        unsafe { objc2_app_kit::NSApplication::sharedApplication(self.view.mtm()).sendAction_to_from(action, target.as_deref(), Some(&**outline)) };
    }

    /// A click on the button of the first unlinked mention in the note named `name` (probe
    /// mode).
    pub fn make_link(&self, name: &str) {
        let index = {
            let rows = self.source.ivars().rows.borrow();
            let mut in_note = false;
            rows.iter().position(|r| match r {
                Row::Note { path, .. } => {
                    in_note = document_name(path) == name;
                    false
                }
                Row::Line { section: Section::Unlinked, .. } => in_note,
                _ => false,
            })
        };
        let Some(index) = index else { return };
        // SAFETY: an item of this outline view.
        let row = unsafe { self.outline.rowForItem(Some(&self.source.object(index))) };
        let cell = (row >= 0).then(|| self.outline.viewAtColumn_row_makeIfNecessary(0, row, true)).flatten().and_then(|v| v.downcast::<NSTableCellView>().ok());
        if let Some(button) = cell.as_deref().and_then(link_button) {
            // SAFETY: a click with no sender, as the button's own.
            unsafe { button.performClick(None) };
        }
    }
}

const CELL_ID: &str = "ConnectionCell";
const LINK_ID: &str = "make-link";

/// A reused cell: a text of up to three lines, and a button that makes a mention a link.
fn cell(outline: &NSOutlineView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    let id = NSString::from_str(CELL_ID);
    // SAFETY: reused views with this identifier are always cells made below.
    let reused = unsafe { outline.makeViewWithIdentifier_owner(&id, None) };
    if let Some(cell) = reused.and_then(|v| v.downcast::<NSTableCellView>().ok()) {
        return cell;
    }
    let cell = NSTableCellView::new(mtm);
    cell.setIdentifier(Some(&id));
    let text = NSTextField::wrappingLabelWithString(ns_string!(""), mtm);
    text.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    // The mention's own font and colour show (a label drops them otherwise).
    text.setAllowsEditingTextAttributes(true);
    if let Some(cell) = text.cell() {
        cell.setTruncatesLastVisibleLine(true);
    }
    text.setTranslatesAutoresizingMaskIntoConstraints(false);
    let button = NSButton::new(mtm);
    button.setBordered(false);
    button.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("link"), Some(&NSString::from_str(strings::MAKE_LINK))).as_deref());
    button.setToolTip(Some(&NSString::from_str(strings::MAKE_LINK)));
    button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    button.setIdentifier(Some(&NSString::from_str(LINK_ID)));
    button.setTranslatesAutoresizingMaskIntoConstraints(false);
    cell.addSubview(&text);
    cell.addSubview(&button);
    for c in [
        text.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 2.0),
        text.topAnchor().constraintEqualToAnchor_constant(&cell.topAnchor(), 3.0),
        text.bottomAnchor().constraintEqualToAnchor_constant(&cell.bottomAnchor(), -3.0),
        text.trailingAnchor().constraintEqualToAnchor_constant(&button.leadingAnchor(), -4.0),
        button.trailingAnchor().constraintEqualToAnchor_constant(&cell.trailingAnchor(), -4.0),
        button.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        button.widthAnchor().constraintEqualToConstant(16.0),
    ] {
        c.setActive(true);
    }
    // SAFETY: the cell's own text field.
    unsafe { cell.setTextField(Some(&text)) };
    cell
}

/// A cell's button that makes a mention a link.
fn link_button(cell: &NSTableCellView) -> Option<Retained<NSButton>> {
    let id = NSString::from_str(LINK_ID);
    cell.subviews().iter().find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id))).and_then(|v| v.downcast::<NSButton>().ok())
}
