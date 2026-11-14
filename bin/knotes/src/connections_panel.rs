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

/// Notes in a section past which they are listed folded (their lines made only when
/// unfolded): a note linked from a thousand others listed every line at once.
const FOLD_OVER: usize = 20;
/// Notes a section lists before a row asking for the rest: unfolding a section of 1232 notes
/// took AppKit 21 ms.
const FIRST_NOTES: usize = 50;

/// What the panel lists for a document.
#[derive(Debug, Default, Clone, PartialEq)]
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
    /// The notes of a section not listed yet, listed on a click.
    More(Section, usize),
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
    /// Sections whose notes are all listed (their "Ще N…" clicked).
    all_of: RefCell<Vec<Section>>,
    /// Asked to list a section in full.
    on_more: RefCell<Option<Box<dyn Fn()>>>,
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
            self.child_count(item.and_then(index_of)) as NSInteger
        }

        #[unsafe(method_id(outlineView:child:ofItem:))]
        fn child(&self, _outline: &NSOutlineView, index: NSInteger, item: Option<&AnyObject>) -> Retained<AnyObject> {
            let child = self.child_at(item.and_then(index_of), index as usize).unwrap_or(0);
            self.object(child)
        }

        #[unsafe(method(outlineView:isItemExpandable:))]
        fn is_expandable(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            index_of(item).is_some_and(|i| self.child_count(Some(i)) > 0)
        }
    }

    unsafe impl NSOutlineViewDelegate for ConnectionsSource {
        /// Heights worked out, not laid out row by row: a section, a note and a link take one
        /// line; a mention's line takes as many as its text wraps to, three at most.
        #[unsafe(method(outlineView:heightOfRowByItem:))]
        fn row_height(&self, outline: &NSOutlineView, item: &AnyObject) -> f64 {
            let Some(i) = index_of(item) else { return ROW_HEIGHT };
            match self.ivars().rows.borrow().get(i) {
                Some(Row::Line { mention, .. }) => {
                    // The text's width: the column less the line's indentation and the button.
                    // SAFETY: an item of this outline view.
                    let level = unsafe { outline.levelForItem(Some(item)) }.max(0) as f64;
                    let column = outline.tableColumns().firstObject().map_or(200.0, |c| c.width());
                    let width = (column - level * outline.indentationPerLevel() - 30.0).max(40.0);
                    line_height(mention, width)
                }
                _ => ROW_HEIGHT,
            }
        }

        /// The panel's width changed: the mentions' lines wrap again.
        #[unsafe(method(outlineViewColumnDidResize:))]
        fn column_did_resize(&self, notification: &objc2_foundation::NSNotification) {
            if let Some(outline) = notification.object().and_then(|o| o.downcast::<NSOutlineView>().ok()) {
                let all = objc2_foundation::NSIndexSet::indexSetWithIndexesInRange(objc2_foundation::NSRange::new(0, outline.numberOfRows().max(0) as usize));
                outline.noteHeightOfRowsWithIndexesChanged(&all);
            }
        }

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
                Row::More(section, _) => {
                    self.ivars().all_of.borrow_mut().push(section);
                    if let Some(callback) = self.ivars().on_more.borrow().as_ref() {
                        callback();
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
    /// How many rows are under row `of` (or at the top), not copying them: the outline view
    /// asks once per row.
    fn child_count(&self, of: Option<usize>) -> usize {
        match of {
            Some(i) => self.ivars().children.borrow().get(i).map_or(0, Vec::len),
            None => self.ivars().roots.borrow().len(),
        }
    }

    fn child_at(&self, of: Option<usize>, index: usize) -> Option<usize> {
        match of {
            Some(i) => self.ivars().children.borrow().get(i)?.get(index).copied(),
            None => self.ivars().roots.borrow().get(index).copied(),
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
            Row::More(_, n) => {
                field.setStringValue(&NSString::from_str(&strings::more_notes(*n)));
                field.setTextColor(Some(&NSColor::linkColor()));
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

/// A row of one line.
const ROW_HEIGHT: f64 = 24.0;
/// The space above and below a mention's line.
const LINE_PADDING: f64 = 6.0;

/// The height of a mention's line wrapped to `width`, three lines at most.
fn line_height(mention: &Mention, width: f64) -> f64 {
    let text = line_text(mention);
    let font = NSFont::systemFontOfSize(NSFont::smallSystemFontSize() + 1.0);
    let line = (font.ascender() - font.descender() + font.leading()).ceil();
    let options = objc2_app_kit::NSStringDrawingOptions::UsesLineFragmentOrigin;
    let tall = objc2_app_kit::NSAttributedStringNSExtendedStringDrawing::boundingRectWithSize_options_context(&*text, objc2_foundation::NSSize::new(width, f64::MAX), options, None).size.height.ceil();
    tall.min(3.0 * line).max(line) + LINE_PADDING
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
    /// What it lists now.
    shown: RefCell<Connections>,
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
            all_of: RefCell::new(Vec::new()),
            on_more: RefCell::new(None),
        });
        let source: Retained<ConnectionsSource> = unsafe { msg_send![super(source), init] };
        let outline = NSOutlineView::new(mtm);
        outline.setStyle(NSTableViewStyle::Plain);
        outline.setHeaderView(None);
        outline.setUsesAutomaticRowHeights(false);
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
        Self { view, shown: RefCell::new(Connections::default()), outline, source }
    }

    /// The notes linking here and naming the document, with what is listed of its own links.
    pub fn set_mentions(&self, linked: Vec<(String, Vec<Mention>)>, unlinked: Vec<(String, Vec<Mention>)>) {
        let outgoing = self.shown.borrow().outgoing.clone();
        self.show(Connections { linked, unlinked, outgoing });
    }

    /// The document's own links, with what is listed of the notes linking here.
    pub fn set_outgoing(&self, outgoing: Vec<Outgoing>) {
        let Connections { linked, unlinked, .. } = self.shown.borrow().clone();
        self.show(Connections { linked, unlinked, outgoing });
    }

    /// Lists `connections`, sections unfolded and their notes too unless there are many; the
    /// same list again changes nothing (the rows keep their folds and the scroll).
    pub fn show(&self, connections: Connections) {
        if *self.shown.borrow() == connections {
            return;
        }
        *self.shown.borrow_mut() = connections.clone();
        let all_of = self.source.ivars().all_of.borrow().clone();
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
            let listed = if all_of.contains(&section) { notes.len() } else { notes.len().min(FIRST_NOTES) };
            for (path, mentions) in &notes[..listed] {
                let note = add(Row::Note { path: path.clone(), count: mentions.len() }, Some(head), &mut rows, &mut children);
                for mention in mentions {
                    add(Row::Line { path: path.clone(), mention: mention.clone(), section }, Some(note), &mut rows, &mut children);
                }
            }
            if listed < notes.len() {
                add(Row::More(section, notes.len() - listed), Some(head), &mut rows, &mut children);
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
        let unfold: Vec<(usize, bool)> = {
            let rows = ivars.rows.borrow();
            let children = ivars.children.borrow();
            ivars.roots.borrow().iter().map(|&head| (head, children[head].len() <= FOLD_OVER && !matches!(rows[head], Row::Section(Section::Outgoing, _)))).collect()
        };
        for (head, notes_too) in unfold {
            // SAFETY: an item of this outline view.
            unsafe { self.outline.expandItem_expandChildren(Some(&self.source.object(head)), notes_too) };
        }
    }

    /// Called when a "Ще N…" row is clicked; the window lists the panel again (`list_again`).
    pub fn on_more(&self, callback: impl Fn() + 'static) {
        *self.source.ivars().on_more.borrow_mut() = Some(Box::new(callback));
    }

    /// Lists the same connections again, the sections asked for in full.
    pub fn relist(&self) {
        self.list_again();
    }

    pub fn on_open(&self, callback: impl Fn(String, Option<Range<usize>>) + 'static) {
        *self.source.ivars().on_open.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_follow(&self, callback: impl Fn(String, Option<String>) + 'static) {
        *self.source.ivars().on_follow.borrow_mut() = Some(Box::new(callback));
    }

    /// Another document: its sections are listed from their first notes again.
    pub fn forget_more(&self) {
        self.source.ivars().all_of.borrow_mut().clear();
    }

    /// A "Ще N…" row clicked: the list is made again, that section in full.
    fn list_again(&self) {
        let connections = std::mem::take(&mut *self.shown.borrow_mut());
        self.show(connections);
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
                    Row::More(_, n) => strings::more_notes(*n),
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

/// A reused cell: a text of up to three lines, and a button that makes a mention a link. Laid
/// out by its frame, not constraints: creating the rows in view was most of listing a note.
fn cell(outline: &NSOutlineView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    use objc2_app_kit::NSAutoresizingMaskOptions as Mask;
    let id = NSString::from_str(CELL_ID);
    // SAFETY: reused views with this identifier are always cells made below.
    let reused = unsafe { outline.makeViewWithIdentifier_owner(&id, None) };
    if let Some(cell) = reused.and_then(|v| v.downcast::<NSTableCellView>().ok()) {
        return cell;
    }
    let frame = objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(0.0, 0.0), objc2_foundation::NSSize::new(200.0, ROW_HEIGHT));
    let cell = NSTableCellView::initWithFrame(NSTableCellView::alloc(mtm), frame);
    cell.setIdentifier(Some(&id));
    let text = NSTextField::wrappingLabelWithString(ns_string!(""), mtm);
    text.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    // The mention's own font and colour show (a label drops them otherwise).
    text.setAllowsEditingTextAttributes(true);
    if let Some(field_cell) = text.cell() {
        field_cell.setTruncatesLastVisibleLine(true);
    }
    text.setFrame(objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(2.0, 3.0), objc2_foundation::NSSize::new(200.0 - 28.0, ROW_HEIGHT - 6.0)));
    text.setAutoresizingMask(Mask::ViewWidthSizable | Mask::ViewHeightSizable);
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(200.0 - 20.0, (ROW_HEIGHT - 16.0) / 2.0), objc2_foundation::NSSize::new(16.0, 16.0)));
    button.setBordered(false);
    LINK_SYMBOL.with(|symbol| button.setImage(symbol.as_deref()));
    button.setToolTip(Some(&NSString::from_str(strings::MAKE_LINK)));
    button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    button.setIdentifier(Some(&NSString::from_str(LINK_ID)));
    button.setAutoresizingMask(Mask::ViewMinXMargin | Mask::ViewMinYMargin | Mask::ViewMaxYMargin);
    cell.addSubview(&text);
    cell.addSubview(&button);
    // SAFETY: the cell's own text field.
    unsafe { cell.setTextField(Some(&text)) };
    cell
}

thread_local! {
    /// The link symbol, looked up once.
    static LINK_SYMBOL: Option<Retained<NSImage>> = NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("link"), Some(&NSString::from_str(strings::MAKE_LINK)));
}

/// A cell's button that makes a mention a link.
fn link_button(cell: &NSTableCellView) -> Option<Retained<NSButton>> {
    let id = NSString::from_str(LINK_ID);
    cell.subviews().iter().find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id))).and_then(|v| v.downcast::<NSButton>().ok())
}
