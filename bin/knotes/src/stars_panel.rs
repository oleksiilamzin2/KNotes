//! The starred folders and files, in the right panel: a list in the order they were starred. A
//! click shows the folder or opens the file; a row's menu takes its star off.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSControlTextEditingDelegate, NSFont, NSImage, NSImageView, NSLineBreakMode, NSMenu, NSMenuDelegate, NSMenuItem, NSOutlineView, NSOutlineViewDataSource, NSOutlineViewDelegate, NSScrollView, NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextAlignment,
    NSTextField, NSUserInterfaceItemIdentification, NSView,
};
use objc2_foundation::{NSInteger, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::strings;

/// A star shown: the star itself (a path; a folder's ends with `/`) and the name it goes by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarRow {
    pub star: String,
    pub name: String,
}

type OnStar = Box<dyn Fn(String)>;

pub struct SourceIvars {
    rows: RefCell<Vec<StarRow>>,
    /// One object per row: the outline view knows items by object.
    objects: RefCell<Vec<Retained<NSNumber>>>,
    /// Called with the star clicked, and with the one whose menu takes it off.
    on_open: RefCell<Option<OnStar>>,
    on_unstar: RefCell<Option<OnStar>>,
    /// The list itself: a menu opening knows nothing of its view.
    outline: RefCell<Option<objc2::rc::Weak<NSOutlineView>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesStarsSource"]
    #[ivars = SourceIvars]
    pub struct StarsSource;

    unsafe impl NSObjectProtocol for StarsSource {}

    unsafe impl NSControlTextEditingDelegate for StarsSource {}

    unsafe impl NSOutlineViewDataSource for StarsSource {
        #[unsafe(method(outlineView:numberOfChildrenOfItem:))]
        fn number_of_children(&self, _outline: &NSOutlineView, item: Option<&AnyObject>) -> NSInteger {
            if item.is_some() { 0 } else { self.ivars().rows.borrow().len() as NSInteger }
        }

        #[unsafe(method_id(outlineView:child:ofItem:))]
        fn child(&self, _outline: &NSOutlineView, index: NSInteger, _item: Option<&AnyObject>) -> Retained<AnyObject> {
            let object = self.ivars().objects.borrow().get(index as usize).cloned().unwrap_or_else(|| NSNumber::new_usize(index as usize));
            Retained::into_super(Retained::into_super(Retained::into_super(object)))
        }

        #[unsafe(method(outlineView:isItemExpandable:))]
        fn is_expandable(&self, _outline: &NSOutlineView, _item: &AnyObject) -> bool {
            false
        }
    }

    unsafe impl NSOutlineViewDelegate for StarsSource {
        #[unsafe(method_id(outlineView:viewForTableColumn:item:))]
        fn view_for_item(&self, outline: &NSOutlineView, _column: Option<&NSTableColumn>, item: &AnyObject) -> Option<Retained<NSView>> {
            let row = index_of(item).and_then(|i| self.ivars().rows.borrow().get(i).cloned());
            row.map(|row| {
                let cell = cell(outline, self.mtm());
                // SAFETY: the cells are built by `cell` with their text field and image view set.
                unsafe {
                    if let Some(field) = cell.textField() {
                        field.setStringValue(&NSString::from_str(&row.name));
                        field.setToolTip(Some(&NSString::from_str(row.star.trim_end_matches('/'))));
                    }
                    if let Some(image) = cell.imageView() {
                        let symbol = if row.star.ends_with('/') { "folder" } else { "doc.text" };
                        image.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None).as_deref());
                    }
                }
                Retained::into_super(cell)
            })
        }
    }

    impl StarsSource {
        /// A click on a star.
        #[unsafe(method(starClicked:))]
        fn star_clicked(&self, sender: &NSOutlineView) {
            // The row clicked; the one selected when the action comes from the keyboard or a probe.
            let row = if sender.clickedRow() >= 0 { sender.clickedRow() } else { sender.selectedRow() };
            if let (Some(star), Some(callback)) = (self.star_at(sender, row), self.ivars().on_open.borrow().as_ref()) {
                callback(star);
            }
        }

        /// The row's menu: its star taken off.
        #[unsafe(method(unstar:))]
        fn unstar(&self, item: &NSMenuItem) {
            // SAFETY: the represented object is the star's path, set when the menu is filled.
            let star = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string());
            if let (Some(star), Some(callback)) = (star, self.ivars().on_unstar.borrow().as_ref()) {
                callback(star);
            }
        }
    }

    unsafe impl NSMenuDelegate for StarsSource {
        /// The menu of the row under the pointer; none below the rows.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            menu.removeAllItems();
            let Some(outline) = self.ivars().outline.borrow().as_ref().and_then(objc2::rc::Weak::load) else { return };
            if let Some(star) = self.star_at(&outline, outline.clickedRow()) {
                self.fill_menu(menu, &star);
            }
        }
    }
);

impl StarsSource {
    fn star_at(&self, outline: &NSOutlineView, row: NSInteger) -> Option<String> {
        let i = (row >= 0).then(|| outline.itemAtRow(row)).flatten().and_then(|item| index_of(&item))?;
        self.ivars().rows.borrow().get(i).map(|r| r.star.clone())
    }

    /// The menu of `star`'s row.
    fn fill_menu(&self, menu: &NSMenu, star: &str) {
        // SAFETY: the source outlives the menu (the panel keeps both) and implements the action.
        unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm()), &NSString::from_str(strings::UNSTAR), Some(sel!(unstar:)), ns_string!(""));
            item.setTarget(Some(self));
            item.setRepresentedObject(Some(&NSString::from_str(star)));
            menu.addItem(&item);
        }
    }
}

/// The row an item object stands for.
fn index_of(object: &AnyObject) -> Option<usize> {
    Some(object.downcast_ref::<NSNumber>()?.as_usize())
}

pub struct StarsPanel {
    pub view: Retained<NSView>,
    outline: Retained<NSOutlineView>,
    /// Kept alive here: the outline view holds its data source weakly.
    source: Retained<StarsSource>,
    /// Shown when nothing is starred.
    empty: Retained<NSTextField>,
}

impl StarsPanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let source = StarsSource::alloc(mtm).set_ivars(SourceIvars { rows: RefCell::new(Vec::new()), objects: RefCell::new(Vec::new()), on_open: RefCell::new(None), on_unstar: RefCell::new(None), outline: RefCell::new(None) });
        let source: Retained<StarsSource> = unsafe { msg_send![super(source), init] };
        let outline = NSOutlineView::new(mtm);
        outline.setStyle(NSTableViewStyle::SourceList);
        outline.setHeaderView(None);
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("star"));
        outline.addTableColumn(&column);
        let menu = NSMenu::new(mtm);
        // SAFETY: the source outlives the outline and its menu (the panel keeps them) and takes
        // the action.
        unsafe {
            outline.setOutlineTableColumn(Some(&column));
            outline.setDataSource(Some(ProtocolObject::from_ref(&*source)));
            outline.setDelegate(Some(ProtocolObject::from_ref(&*source)));
            outline.setTarget(Some(&source));
            outline.setAction(Some(sel!(starClicked:)));
            menu.setDelegate(Some(ProtocolObject::from_ref(&*source)));
        }
        // SAFETY: the menu is the outline's own from here on.
        unsafe { outline.setMenu(Some(&menu)) };
        *source.ivars().outline.borrow_mut() = Some(objc2::rc::Weak::new(&*outline));
        // Without the line the toolbar draws over a scroll view under it (ADR 0016).
        let scroll: Retained<NSScrollView> = Retained::into_super(crate::quiet_scroll_view::QuietScrollView::new(mtm));
        scroll.setDocumentView(Some(&outline));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        // A little room above the first row, as the sidebar has.
        scroll.setAdditionalSafeAreaInsets(objc2_foundation::NSEdgeInsets { top: 8.0, left: 0.0, bottom: 0.0, right: 0.0 });
        let empty = NSTextField::wrappingLabelWithString(&NSString::new(), mtm);
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

    /// Lists `rows`; says how to star when there are none.
    pub fn show(&self, rows: Vec<StarRow>) {
        let ivars = self.source.ivars();
        self.empty.setStringValue(&NSString::from_str(if rows.is_empty() { strings::STARS_EMPTY } else { "" }));
        if *ivars.rows.borrow() == rows {
            return;
        }
        *ivars.objects.borrow_mut() = (0..rows.len()).map(NSNumber::new_usize).collect();
        *ivars.rows.borrow_mut() = rows;
        self.outline.reloadData();
    }

    /// Called with the star clicked.
    pub fn on_open(&self, callback: impl Fn(String) + 'static) {
        *self.source.ivars().on_open.borrow_mut() = Some(Box::new(callback));
    }

    /// Called with the star its row's menu takes off.
    pub fn on_unstar(&self, callback: impl Fn(String) + 'static) {
        *self.source.ivars().on_unstar.borrow_mut() = Some(Box::new(callback));
    }

    fn row_named(&self, name: &str) -> Option<NSInteger> {
        self.source.ivars().rows.borrow().iter().position(|r| r.name == name).map(|i| i as NSInteger)
    }

    /// A click on the row named `name` (probe mode): the row selected and the outline view's
    /// action sent to its target, as AppKit does after a click.
    pub fn click(&self, name: &str) {
        let (Some(row), Some(action)) = (self.row_named(name), self.outline.action()) else { return };
        self.outline.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row as usize), false);
        let target = self.outline.target();
        // SAFETY: the outline view's own action and target, with it as the sender.
        unsafe { objc2_app_kit::NSApplication::sharedApplication(self.view.mtm()).sendAction_to_from(action, target.as_deref(), Some(&**self.outline)) };
    }

    /// The menu of the row named `name` filled and its item chosen, as a click would (probe
    /// mode); the menu's items.
    pub fn unstar(&self, name: &str) -> String {
        let (Some(row), Some(menu)) = (self.row_named(name), self.outline.menu()) else { return String::new() };
        let Some(star) = self.source.star_at(&self.outline, row) else { return String::new() };
        menu.removeAllItems();
        self.source.fill_menu(&menu, &star);
        let items = menu.itemArray();
        if let Some(item) = items.firstObject() {
            // SAFETY: the menu item's own target and action, sent as a click would.
            unsafe { objc2_app_kit::NSApplication::sharedApplication(self.view.mtm()).sendAction_to_from(item.action().expect("menu action"), item.target().as_deref(), Some(&item)) };
        }
        items.iter().map(|i| i.title().to_string()).collect::<Vec<_>>().join(" | ")
    }

    /// The rows shown (a folder's with `/` after its name), and what the panel says with none
    /// (probe mode).
    pub fn describe(&self) -> (String, String) {
        let rows = self.source.ivars().rows.borrow();
        let shown: Vec<String> = (0..self.outline.numberOfRows())
            .filter_map(|row| {
                let r = rows.get(index_of(&*self.outline.itemAtRow(row)?)?)?;
                Some(format!("{}{}", r.name, if r.star.ends_with('/') { "/" } else { "" }))
            })
            .collect();
        (shown.join(" | "), self.empty.stringValue().to_string())
    }
}

const CELL_ID: &str = "StarCell";

/// A reused cell: a folder's or a file's picture and its name on one line.
fn cell(outline: &NSOutlineView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    let id = NSString::from_str(CELL_ID);
    // SAFETY: reused views with this identifier are always cells made below.
    let reused = unsafe { outline.makeViewWithIdentifier_owner(&id, None) };
    if let Some(cell) = reused.and_then(|v| v.downcast::<NSTableCellView>().ok()) {
        return cell;
    }
    let cell = NSTableCellView::new(mtm);
    cell.setIdentifier(Some(&id));
    let image = NSImageView::new(mtm);
    let title = NSTextField::labelWithString(ns_string!(""), mtm);
    title.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
    title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    for v in [&*image as &NSView, &title] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        cell.addSubview(v);
    }
    for c in [
        image.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 2.0),
        image.widthAnchor().constraintEqualToConstant(18.0),
        image.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        title.leadingAnchor().constraintEqualToAnchor_constant(&image.trailingAnchor(), 6.0),
        title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&cell.trailingAnchor(), -4.0),
        title.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
    ] {
        c.setActive(true);
    }
    // SAFETY: the cell's own text field and image view.
    unsafe {
        cell.setTextField(Some(&title));
        cell.setImageView(Some(&image));
    }
    cell
}
