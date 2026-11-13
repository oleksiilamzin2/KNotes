//! The outline of the document open, in the right panel (ADR 0023): an `NSOutlineView` over
//! `knotes_app::Outline`, headings nested by level and folding.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use knotes_app::{Outline, OutlineEntry};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSControlTextEditingDelegate, NSFont, NSLineBreakMode, NSOutlineView, NSOutlineViewDataSource, NSOutlineViewDelegate, NSScrollView, NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextAlignment, NSTextField,
    NSUserInterfaceItemIdentification, NSView,
};
use objc2_foundation::{NSInteger, NSNotification, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string};

type OnChoose = Box<dyn Fn(usize)>;

pub struct SourceIvars {
    model: RefCell<Outline>,
    /// One object per heading: the outline view knows items by object.
    objects: RefCell<Vec<Retained<NSNumber>>>,
    /// The keys of the headings folded by the user, kept while the document stays open.
    folded: RefCell<HashSet<String>>,
    /// Set while the panel folds, unfolds or selects rows itself.
    quiet: Cell<bool>,
    /// Called with the heading clicked (index).
    on_choose: RefCell<Option<OnChoose>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesOutlineSource"]
    #[ivars = SourceIvars]
    pub struct OutlineSource;

    unsafe impl NSObjectProtocol for OutlineSource {}

    unsafe impl NSControlTextEditingDelegate for OutlineSource {}

    unsafe impl NSOutlineViewDataSource for OutlineSource {
        #[unsafe(method(outlineView:numberOfChildrenOfItem:))]
        fn number_of_children(&self, _outline: &NSOutlineView, item: Option<&AnyObject>) -> NSInteger {
            self.ivars().model.borrow().children(item.and_then(index_of)).len() as NSInteger
        }

        #[unsafe(method_id(outlineView:child:ofItem:))]
        fn child(&self, _outline: &NSOutlineView, index: NSInteger, item: Option<&AnyObject>) -> Retained<AnyObject> {
            let child = self.ivars().model.borrow().children(item.and_then(index_of)).get(index as usize).copied().unwrap_or(0);
            self.object(child)
        }

        #[unsafe(method(outlineView:isItemExpandable:))]
        fn is_expandable(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            index_of(item).is_some_and(|i| !self.ivars().model.borrow().children(Some(i)).is_empty())
        }
    }

    unsafe impl NSOutlineViewDelegate for OutlineSource {
        #[unsafe(method_id(outlineView:viewForTableColumn:item:))]
        fn view_for_item(&self, outline: &NSOutlineView, _column: Option<&NSTableColumn>, item: &AnyObject) -> Option<Retained<NSView>> {
            let title = index_of(item).and_then(|i| self.ivars().model.borrow().entries().get(i).map(|e| e.title.clone()));
            title.map(|title| {
                let cell = cell(outline, self.mtm());
                // SAFETY: the cells are built by `cell` with their text field set.
                if let Some(field) = unsafe { cell.textField() } {
                    field.setStringValue(&NSString::from_str(&title));
                    field.setToolTip(Some(&NSString::from_str(&title)));
                }
                Retained::into_super(cell)
            })
        }

        #[unsafe(method(outlineViewItemDidCollapse:))]
        fn did_collapse(&self, notification: &NSNotification) {
            self.folding_changed(notification, true);
        }

        #[unsafe(method(outlineViewItemDidExpand:))]
        fn did_expand(&self, notification: &NSNotification) {
            self.folding_changed(notification, false);
        }
    }

    impl OutlineSource {
        /// A click on a heading.
        #[unsafe(method(headingClicked:))]
        fn heading_clicked(&self, sender: &NSOutlineView) {
            // The row clicked; the one selected when the action comes from the keyboard or a probe.
            let row = if sender.clickedRow() >= 0 { sender.clickedRow() } else { sender.selectedRow() };
            let heading = (row >= 0).then(|| sender.itemAtRow(row)).flatten().and_then(|item| index_of(&item));
            if let (Some(heading), Some(callback)) = (heading, self.ivars().on_choose.borrow().as_ref()) {
                callback(heading);
            }
        }
    }
);

impl OutlineSource {
    fn object(&self, i: usize) -> Retained<AnyObject> {
        let object = self.ivars().objects.borrow().get(i).cloned().unwrap_or_else(|| NSNumber::new_usize(i));
        Retained::into_super(Retained::into_super(Retained::into_super(object)))
    }

    /// The user folded or unfolded a heading: remembered by its key.
    fn folding_changed(&self, notification: &NSNotification, folded: bool) {
        if self.ivars().quiet.get() {
            return;
        }
        // SAFETY: the notification's user info holds the item under "NSObject".
        let item = notification.userInfo().and_then(|info| info.objectForKey(ns_string!("NSObject")));
        let Some(i) = item.as_deref().and_then(index_of) else { return };
        let key = self.ivars().model.borrow().key(i);
        let mut set = self.ivars().folded.borrow_mut();
        if folded {
            set.insert(key);
        } else {
            set.remove(&key);
        }
    }
}

/// The heading an item object stands for.
fn index_of(object: &AnyObject) -> Option<usize> {
    Some(object.downcast_ref::<NSNumber>()?.as_usize())
}

pub struct OutlinePanel {
    pub view: Retained<NSView>,
    outline: Retained<NSOutlineView>,
    /// Kept alive here: the outline view holds its data source weakly.
    source: Retained<OutlineSource>,
    /// Shown when the document has no headings.
    empty: Retained<NSTextField>,
}

impl OutlinePanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let source = OutlineSource::alloc(mtm).set_ivars(SourceIvars {
            model: RefCell::new(Outline::default()),
            objects: RefCell::new(Vec::new()),
            folded: RefCell::new(HashSet::new()),
            quiet: Cell::new(false),
            on_choose: RefCell::new(None),
        });
        let source: Retained<OutlineSource> = unsafe { msg_send![super(source), init] };
        let outline = NSOutlineView::new(mtm);
        outline.setStyle(NSTableViewStyle::SourceList);
        outline.setHeaderView(None);
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("heading"));
        outline.addTableColumn(&column);
        // SAFETY: the source outlives the outline (the panel keeps both) and takes the action.
        unsafe {
            outline.setOutlineTableColumn(Some(&column));
            outline.setDataSource(Some(ProtocolObject::from_ref(&*source)));
            outline.setDelegate(Some(ProtocolObject::from_ref(&*source)));
            outline.setTarget(Some(&source));
            outline.setAction(Some(sel!(headingClicked:)));
        }
        let scroll = NSScrollView::new(mtm);
        scroll.setDocumentView(Some(&outline));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        let empty = NSTextField::labelWithString(&NSString::new(), mtm);
        empty.setTextColor(Some(&NSColor::secondaryLabelColor()));
        empty.setAlignment(NSTextAlignment::Center);
        let view = NSView::new(mtm);
        for v in [&*scroll as &NSView, &empty] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        }
        let guide = view.safeAreaLayoutGuide();
        for c in [
            scroll.topAnchor().constraintEqualToAnchor(&guide.topAnchor()),
            scroll.bottomAnchor().constraintEqualToAnchor(&view.bottomAnchor()),
            scroll.leadingAnchor().constraintEqualToAnchor(&view.leadingAnchor()),
            scroll.trailingAnchor().constraintEqualToAnchor(&view.trailingAnchor()),
            empty.topAnchor().constraintEqualToAnchor_constant(&guide.topAnchor(), 24.0),
            empty.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 16.0),
            empty.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -16.0),
        ] {
            c.setActive(true);
        }
        Self { view, outline, source, empty }
    }

    /// Lists `outline`; `empty` is said when it has no headings. Of the same document
    /// (`same`), folds are kept, and an outline with the same headings in the same places in
    /// the tree only moves them (no reload: the rows stay as they are).
    pub fn show(&self, outline: Outline, empty: &str, same: bool) {
        let ivars = self.source.ivars();
        if !same {
            ivars.folded.borrow_mut().clear();
        }
        let unchanged = {
            let model = ivars.model.borrow();
            same && model.entries().len() == outline.entries().len() && model.entries().iter().zip(outline.entries()).all(|(a, b)| a.level == b.level && a.title == b.title)
        };
        *ivars.model.borrow_mut() = outline;
        self.empty.setStringValue(&NSString::from_str(if ivars.model.borrow().is_empty() { empty } else { "" }));
        if unchanged {
            return;
        }
        *ivars.objects.borrow_mut() = (0..ivars.model.borrow().entries().len()).map(NSNumber::new_usize).collect();
        ivars.quiet.set(true);
        self.outline.reloadData();
        // Unfolded, but for what the user folded.
        let unfold: Vec<usize> = {
            let model = ivars.model.borrow();
            let folded = ivars.folded.borrow();
            (0..model.entries().len()).filter(|&i| !model.children(Some(i)).is_empty() && !folded.contains(&model.key(i))).collect()
        };
        for i in unfold {
            // SAFETY: an item of this outline view.
            unsafe { self.outline.expandItem(Some(&self.source.object(i))) };
        }
        ivars.quiet.set(false);
    }

    /// Called with the heading clicked (index into the outline's entries).
    pub fn on_choose(&self, callback: impl Fn(usize) + 'static) {
        *self.source.ivars().on_choose.borrow_mut() = Some(Box::new(callback));
    }

    /// Heading `i` of the outline shown.
    pub fn entry(&self, i: usize) -> Option<OutlineEntry> {
        self.source.ivars().model.borrow().entries().get(i).cloned()
    }

    /// A click on the row titled `title` (probe mode): the row selected and the outline
    /// view's action sent to its target, as AppKit does after a click (the probe's window is not
    /// key, and the outline view tracks no click there).
    pub fn click(&self, title: &str) {
        let outline = &self.outline;
        let row = {
            let model = self.source.ivars().model.borrow();
            (0..outline.numberOfRows()).find(|&row| outline.itemAtRow(row).and_then(|item| index_of(&item)).is_some_and(|i| model.entries()[i].title == title))
        };
        let (Some(row), Some(action)) = (row, outline.action()) else { return };
        outline.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row as usize), false);
        let target = outline.target();
        // SAFETY: the outline view's own action and target, with it as the sender.
        unsafe { objc2_app_kit::NSApplication::sharedApplication(self.view.mtm()).sendAction_to_from(action, target.as_deref(), Some(&**outline)) };
    }

    /// The rows shown, a dot per level under the top, and what the panel says with no
    /// headings (probe mode).
    pub fn describe(&self) -> (String, String) {
        let model = self.source.ivars().model.borrow();
        let rows: Vec<String> = (0..self.outline.numberOfRows())
            .filter_map(|row| {
                let i = index_of(&*self.outline.itemAtRow(row)?)?;
                Some(format!("{}{}", "·".repeat(self.outline.levelForRow(row).max(0) as usize), model.entries().get(i)?.title))
            })
            .collect();
        (rows.join(" | "), self.empty.stringValue().to_string())
    }
}

const CELL_ID: &str = "OutlineCell";

/// A reused cell: the heading's title on one line.
fn cell(outline: &NSOutlineView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    let id = NSString::from_str(CELL_ID);
    // SAFETY: reused views with this identifier are always cells made below.
    let reused = unsafe { outline.makeViewWithIdentifier_owner(&id, None) };
    if let Some(cell) = reused.and_then(|v| v.downcast::<NSTableCellView>().ok()) {
        return cell;
    }
    let cell = NSTableCellView::new(mtm);
    cell.setIdentifier(Some(&id));
    let title = NSTextField::labelWithString(ns_string!(""), mtm);
    title.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
    title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    title.setTranslatesAutoresizingMaskIntoConstraints(false);
    cell.addSubview(&title);
    for c in [
        title.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 2.0),
        title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&cell.trailingAnchor(), -4.0),
        title.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
    ] {
        c.setActive(true);
    }
    // SAFETY: the cell's own text field.
    unsafe { cell.setTextField(Some(&title)) };
    cell
}
