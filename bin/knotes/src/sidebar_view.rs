//! Sidebar: an `NSOutlineView` (source list style) over the `knotes_app::Sidebar` model. A
//! note dragged from the list onto a folder moves there, and a folder is renamed in its row
//! from the context menu (ADR 0019).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use knotes_app::{Sidebar, SidebarItem};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSColor, NSControl, NSEvent, NSResponder, NSTrackingArea, NSTrackingAreaOptions, NSControlTextEditingDelegate, NSDragOperation, NSMenu, NSMenuDelegate, NSMenuItem, NSMenuItemValidation, NSTextFieldDelegate, NSTextView, NSDraggingInfo, NSFont, NSImage, NSImageView, NSOutlineView, NSOutlineViewDataSource,
    NSOutlineViewDelegate, NSPasteboard, NSScrollView, NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextField, NSUserInterfaceItemIdentification,
    NSView,
};
use objc2_foundation::{NSArray, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, NSZeroRect, ns_string};

use crate::note_list_view::NOTE_PATH_TYPE;
use crate::strings;

type OnSelect = Box<dyn Fn(SidebarItem)>;
type OnDrop = Box<dyn Fn(String, String)>;
type OnClose = Box<dyn Fn(String)>;

pub struct Ivars {
    model: RefCell<Sidebar>,
    /// Called with the newly selected item.
    on_select: RefCell<Option<OnSelect>>,
    /// Called with the path of a note dropped on a folder, and the folder (ADR 0019).
    on_drop: RefCell<Option<OnDrop>>,
    /// Called with a folder's path and the name typed in its row.
    on_rename: RefCell<Option<OnDrop>>,
    /// Called with the path of a tab whose cross was clicked (ADR 0024), or of the tab whose
    /// menu closes the others.
    on_close: RefCell<Option<OnClose>>,
    on_close_others: RefCell<Option<OnClose>>,
    outline: RefCell<Weak<NSOutlineView>>,
    /// The folder whose row name is being edited, and that name field.
    renaming: RefCell<Option<(String, Retained<NSTextField>)>>,
    /// One object per item key: the outline view identifies items by object, so reusing them
    /// keeps expansion and selection across reloads.
    objects: RefCell<HashMap<String, Retained<NSString>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSidebarSource"]
    #[ivars = Ivars]
    pub struct SidebarSource;

    unsafe impl NSObjectProtocol for SidebarSource {}

    unsafe impl NSOutlineViewDataSource for SidebarSource {
        #[unsafe(method(outlineView:numberOfChildrenOfItem:))]
        fn number_of_children(&self, _outline: &NSOutlineView, item: Option<&AnyObject>) -> NSInteger {
            let item = item.and_then(item_of);
            self.ivars().model.borrow().child_count(item.as_ref()) as NSInteger
        }

        #[unsafe(method_id(outlineView:child:ofItem:))]
        fn child(&self, _outline: &NSOutlineView, index: NSInteger, item: Option<&AnyObject>) -> Retained<AnyObject> {
            let parent = item.and_then(item_of);
            let child = self.ivars().model.borrow().child(parent.as_ref(), index as usize).cloned();
            self.object_for(&child.unwrap_or(SidebarItem::AllNotes))
        }

        #[unsafe(method(outlineView:isItemExpandable:))]
        fn is_expandable(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            item_of(item).is_some_and(|i| self.ivars().model.borrow().child_count(Some(&i)) > 0)
        }

        /// A dragged note can be dropped on a folder (the drop goes on the folder itself).
        #[unsafe(method(outlineView:validateDrop:proposedItem:proposedChildIndex:))]
        fn validate_drop(&self, outline: &NSOutlineView, _info: &ProtocolObject<dyn NSDraggingInfo>, item: Option<&AnyObject>, _index: NSInteger) -> NSDragOperation {
            if item.and_then(item_of).is_some_and(|i| drop_folder(&i).is_some()) {
                // SAFETY: an item of this outline view; -1 drops on it rather than between rows.
                unsafe { outline.setDropItem_dropChildIndex(item, -1) };
                NSDragOperation::Move
            } else {
                NSDragOperation::None
            }
        }

        #[unsafe(method(outlineView:acceptDrop:item:childIndex:))]
        fn accept_drop(&self, _outline: &NSOutlineView, info: &ProtocolObject<dyn NSDraggingInfo>, item: Option<&AnyObject>, _index: NSInteger) -> bool {
            item.and_then(item_of).is_some_and(|item| self.accept_note_drop(&info.draggingPasteboard(), &item))
        }
    }

    unsafe impl NSControlTextEditingDelegate for SidebarSource {
        /// Enter (or a click elsewhere) renames the folder.
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

    unsafe impl NSTextFieldDelegate for SidebarSource {}

    unsafe impl NSMenuItemValidation for SidebarSource {
        /// Only folders are renamed here; a tab's items are on whenever they show.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            item.action() != Some(sel!(renameFolder:)) || self.menu_folder().is_some()
        }
    }

    unsafe impl NSMenuDelegate for SidebarSource {
        /// The row's menu: a folder's rename, or a tab's closing.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            let outline = self.ivars().outline.borrow().load();
            let row = outline.as_ref().map_or(-1, |o| o.clickedRow());
            let item = outline.filter(|_| row >= 0).and_then(|o| o.itemAtRow(row)).and_then(|o| item_of(&o));
            self.fill_menu(menu, item.as_ref());
        }
    }

    impl SidebarSource {
        /// A tab's menu: that tab closes.
        #[unsafe(method(closeTabFromMenu:))]
        fn close_tab_from_menu(&self, item: &NSMenuItem) {
            if let (Some(path), Some(callback)) = (menu_tab(item), self.ivars().on_close.borrow().as_ref()) {
                callback(path);
            }
        }

        /// A tab's menu: every other tab closes.
        #[unsafe(method(closeOtherTabs:))]
        fn close_other_tabs(&self, item: &NSMenuItem) {
            if let (Some(path), Some(callback)) = (menu_tab(item), self.ivars().on_close_others.borrow().as_ref()) {
                callback(path);
            }
        }

        /// A tab's cross: the tab closes.
        #[unsafe(method(closeTab:))]
        fn close_tab(&self, sender: &NSView) {
            let outline = self.ivars().outline.borrow().load();
            let row = outline.as_ref().map_or(-1, |o| o.rowForView(sender));
            let item = outline.filter(|_| row >= 0).and_then(|o| o.itemAtRow(row)).and_then(|o| item_of(&o));
            if let (Some(SidebarItem::Tab(path)), Some(callback)) = (item, self.ivars().on_close.borrow().as_ref()) {
                callback(path);
            }
        }

        #[unsafe(method(renameFolder:))]
        fn rename_folder(&self, _sender: Option<&AnyObject>) {
            if let Some((row, _)) = self.menu_folder() {
                self.begin_rename(row);
            }
        }
    }

    unsafe impl NSOutlineViewDelegate for SidebarSource {
        #[unsafe(method_id(outlineView:viewForTableColumn:item:))]
        fn view_for_item(&self, outline: &NSOutlineView, _column: Option<&NSTableColumn>, item: &AnyObject) -> Option<Retained<NSView>> {
            self.cell_for(outline, item)
        }

        #[unsafe(method(outlineView:heightOfRowByItem:))]
        fn row_height(&self, outline: &NSOutlineView, item: &AnyObject) -> f64 {
            if item_of(item) == Some(SidebarItem::Gap) { GAP_HEIGHT } else { outline.rowHeight() }
        }

        #[unsafe(method(outlineView:shouldSelectItem:))]
        fn should_select(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            item_of(item) != Some(SidebarItem::Gap)
        }

        #[unsafe(method(outlineViewSelectionDidChange:))]
        fn selection_did_change(&self, notification: &NSNotification) {
            let Some(outline) = notification.object().and_then(|o| o.downcast::<NSOutlineView>().ok()) else { return };
            let row = outline.selectedRow();
            let item = (row >= 0).then(|| outline.itemAtRow(row)).flatten().and_then(|o| item_of(&o));
            if let (Some(item), Some(callback)) = (item, self.ivars().on_select.borrow().as_ref()) {
                callback(item);
            }
        }
    }
);

impl SidebarSource {
    pub fn new(mtm: MainThreadMarker, model: Sidebar) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            model: RefCell::new(model),
            on_select: RefCell::new(None),
            on_drop: RefCell::new(None),
            on_rename: RefCell::new(None),
            on_close: RefCell::new(None),
            on_close_others: RefCell::new(None),
            outline: RefCell::new(Weak::default()),
            renaming: RefCell::new(None),
            objects: RefCell::new(HashMap::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

fn cell_for(&self, outline: &NSOutlineView, item: &AnyObject) -> Option<Retained<NSView>> {
        let item = item_of(item)?;
        let model = self.ivars().model.borrow();
        let (label, icon) = match &item {
            // Empty space between the fixed rows and the tree.
            SidebarItem::Gap => return Some(NSView::new(self.mtm())),
            SidebarItem::AllNotes => (strings::ALL_NOTES.to_owned(), Some("tray.full")),
            SidebarItem::Untagged => (strings::UNTAGGED.to_owned(), Some("tag.slash")),
            SidebarItem::Folder(_) if model.inbox() == Some(&item) => (model.name(&item).to_owned(), Some("tray.and.arrow.down")),
            SidebarItem::Folder(_) => (model.name(&item).to_owned(), Some("folder")),
            SidebarItem::Tag(_) => (model.name(&item).to_owned(), Some("number")),
            SidebarItem::Tab(path) => (model.name(&item).to_owned(), Some(if path.to_lowercase().ends_with(".pdf") { "doc.richtext" } else { "doc.text" })),
        };
        let cell = cell(outline, self.mtm());
        // SAFETY: the cell views are built by `cell` with these outlets set.
        if let Some(field) = unsafe { cell.textField() } {
            field.setStringValue(&NSString::from_str(&label));
        }
        if let Some(image) = unsafe { cell.imageView() } {
            image.setImage(icon.and_then(|name| NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)).as_deref());
            image.setHidden(icon.is_none());
        }
        if let Some(count) = count_field(&cell) {
            let text = model.count(&item).map(|n| n.to_string()).unwrap_or_default();
            count.setStringValue(&NSString::from_str(&text));
        }
        if let Some(close) = close_button(&cell) {
            // A tab's cross shows with the pointer over its row (as Safari's).
            if let Some(hover) = cell.downcast_ref::<SidebarCell>() {
                hover.ivars().closable.set(matches!(item, SidebarItem::Tab(_)));
                hover.show_cross_if_pointed();
            }
            // SAFETY: the source outlives the outline's cells (it is the outline's data source).
            unsafe {
                close.setTarget(Some(self));
                close.setAction(Some(sel!(closeTab:)));
            }
        }
        Some(Retained::into_super(cell))
    }

    pub fn on_select(&self, callback: impl Fn(SidebarItem) + 'static) {
        *self.ivars().on_select.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_drop(&self, callback: impl Fn(String, String) + 'static) {
        *self.ivars().on_drop.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_close(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_close.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_close_others(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_close_others.borrow_mut() = Some(Box::new(callback));
    }

    /// Fills the context menu for `item`'s row: a folder's rename, a tab's closing, nothing
    /// for other rows. Each item knows its row's item.
    pub fn fill_menu(&self, menu: &NSMenu, item: Option<&SidebarItem>) {
        menu.removeAllItems();
        let entries: &[(&str, Sel)] = match item {
            Some(SidebarItem::Folder(_)) => &[(strings::RENAME_NOTE, sel!(renameFolder:))],
            Some(SidebarItem::Tab(_)) => &[(strings::CLOSE_TAB, sel!(closeTabFromMenu:)), (strings::CLOSE_OTHER_TABS, sel!(closeOtherTabs:))],
            _ => &[],
        };
        for &(title, action) in entries {
            // SAFETY: the source outlives the menu (the outline keeps it) and implements the action.
            let entry = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm()), &NSString::from_str(title), Some(action), ns_string!("")) };
            // SAFETY: as above.
            unsafe {
                entry.setTarget(Some(self));
                entry.setRepresentedObject(item.map(|i| NSString::from_str(&i.key())).as_deref().map(|k| k.as_ref()));
            }
            menu.addItem(&entry);
        }
    }

    pub fn on_rename(&self, callback: impl Fn(String, String) + 'static) {
        *self.ivars().on_rename.borrow_mut() = Some(Box::new(callback));
    }

    /// The folder row the context menu was opened on (else the selected row), and its path.
    fn menu_folder(&self) -> Option<(isize, String)> {
        let outline = self.ivars().outline.borrow().load()?;
        let row = if outline.clickedRow() >= 0 { outline.clickedRow() } else { outline.selectedRow() };
        let item = (row >= 0).then(|| outline.itemAtRow(row)).flatten().and_then(|o| item_of(&o))?;
        drop_folder(&item).map(|folder| (row, folder.to_owned()))
    }

    /// Lets the name of the folder in `row` be edited in place, all of it selected.
    pub fn begin_rename(&self, row: isize) {
        let Some(outline) = self.ivars().outline.borrow().load() else { return };
        let Some(folder) = outline.itemAtRow(row).and_then(|o| item_of(&o)).and_then(|i| drop_folder(&i).map(str::to_owned)) else { return };
        let cell = outline.viewAtColumn_row_makeIfNecessary(0, row, true).and_then(|v| v.downcast::<NSTableCellView>().ok());
        // SAFETY: the cell views are built by `cell` with their text field set.
        let Some(name) = cell.and_then(|cell| unsafe { cell.textField() }) else { return };
        name.setEditable(true);
        name.setSelectable(true);
        // SAFETY: the source outlives the outline's cells (it is the outline's data source).
        unsafe { name.setDelegate(Some(ProtocolObject::from_ref(self))) };
        *self.ivars().renaming.borrow_mut() = Some((folder, name.clone()));
        if let Some(window) = outline.window() {
            window.makeFirstResponder(Some(&name));
        }
    }

    /// The name field being edited, for probe mode.
    pub fn renaming_field(&self) -> Option<Retained<NSTextField>> {
        self.ivars().renaming.borrow().as_ref().map(|(_, field)| field.clone())
    }

    /// Ends editing a folder's name: renames it when `commit` and the name changed, otherwise
    /// shows its name again.
    fn finish_rename(&self, commit: bool) {
        let Some((folder, field)) = self.ivars().renaming.borrow_mut().take() else { return };
        let name = field.stringValue().to_string();
        field.setEditable(false);
        field.setSelectable(false);
        let old = folder.rsplit('/').next().unwrap_or(&folder).to_owned();
        let outline = self.ivars().outline.borrow().load();
        if let Some(window) = outline.as_ref().and_then(|o| o.window()) {
            window.makeFirstResponder(outline.as_deref().map(|o| o.as_ref()));
        }
        if !commit || name.trim().is_empty() || name == old {
            field.setStringValue(&NSString::from_str(&old));
            return;
        }
        if let Some(callback) = self.ivars().on_rename.borrow().as_ref() {
            callback(folder, name);
        }
    }

    /// A note dragged from the list (its path on `pasteboard`) dropped on `item`: moves it
    /// there when `item` is a folder.
    pub fn accept_note_drop(&self, pasteboard: &NSPasteboard, item: &SidebarItem) -> bool {
        let (Some(folder), Some(path)) = (drop_folder(item), pasteboard.stringForType(&NSString::from_str(NOTE_PATH_TYPE))) else { return false };
        match self.ivars().on_drop.borrow().as_ref() {
            Some(callback) => {
                callback(path.to_string(), folder.to_owned());
                true
            }
            None => false,
        }
    }

    pub fn set_model(&self, model: Sidebar) {
        *self.ivars().model.borrow_mut() = model;
    }

    pub fn object_for(&self, item: &SidebarItem) -> Retained<AnyObject> {
        let mut objects = self.ivars().objects.borrow_mut();
        let object = objects.entry(item.key()).or_insert_with(|| NSString::from_str(&item.key()));
        Retained::into_super(Retained::into_super(object.clone()))
    }
}

/// The tab a menu item was made for.
fn menu_tab(item: &NSMenuItem) -> Option<String> {
    let key = item.representedObject()?.downcast::<NSString>().ok()?.to_string();
    match SidebarItem::from_key(&key)? {
        SidebarItem::Tab(path) => Some(path),
        _ => None,
    }
}

/// The folder a note dropped on `item` moves to; only folders take notes.
pub fn drop_folder(item: &SidebarItem) -> Option<&str> {
    match item {
        SidebarItem::Folder(path) => Some(path),
        _ => None,
    }
}

/// The sidebar item an outline item object stands for.
pub fn item_of(object: &AnyObject) -> Option<SidebarItem> {
    SidebarItem::from_key(&object.downcast_ref::<NSString>()?.to_string())
}

/// Builds the outline view inside a scroll view; `source` must outlive it (the outline view
/// keeps only weak references to its data source and delegate).
pub fn outline(mtm: MainThreadMarker, source: &SidebarSource) -> (Retained<NSScrollView>, Retained<NSOutlineView>) {
    let outline = NSOutlineView::new(mtm);
    outline.setStyle(NSTableViewStyle::SourceList);
    outline.setHeaderView(None);
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("main"));
    outline.addTableColumn(&column);
    unsafe { outline.setOutlineTableColumn(Some(&column)) };
    unsafe {
        outline.setDataSource(Some(ProtocolObject::from_ref(source)));
        outline.setDelegate(Some(ProtocolObject::from_ref(source)));
    }
    outline.setFloatsGroupRows(false);
    outline.registerForDraggedTypes(&NSArray::from_retained_slice(&[NSString::from_str(NOTE_PATH_TYPE)]));
    *source.ivars().outline.borrow_mut() = Weak::new(&outline);
    // Filled for the row clicked as it opens.
    let menu = NSMenu::new(mtm);
    menu.setDelegate(Some(ProtocolObject::from_ref(source)));
    // SAFETY: a menu built here, kept by the outline.
    unsafe { outline.setMenu(Some(&menu)) };
    let scroll = NSScrollView::new(mtm);
    scroll.setDocumentView(Some(&outline));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    (scroll, outline)
}

const CELL_ID: &str = "SidebarCell";
/// Height of the empty row between the fixed rows and the tree.
const GAP_HEIGHT: f64 = 10.0;
const COUNT_ID: &str = "count";
const CLOSE_ID: &str = "close";

/// A reused cell (icon, name, count on the right), laid out with constraints.
fn cell(outline: &NSOutlineView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    let id = NSString::from_str(CELL_ID);
    // SAFETY: reused views with this identifier are always cells made below.
    let reused = unsafe { outline.makeViewWithIdentifier_owner(&id, None) };
    if let Some(cell) = reused.and_then(|v| v.downcast::<NSTableCellView>().ok()) {
        return cell;
    }
    let cell = SidebarCell::new(mtm);
    cell.setIdentifier(Some(&id));
    let image = NSImageView::new(mtm);
    let name = NSTextField::labelWithString(ns_string!(""), mtm);
    let count = NSTextField::labelWithString(ns_string!(""), mtm);
    count.setIdentifier(Some(&NSString::from_str(COUNT_ID)));
    count.setTextColor(Some(&NSColor::secondaryLabelColor()));
    count.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(NSFont::systemFontSize(), 0.0)));
    // A tab's cross (ADR 0024); hidden on other rows.
    let close = NSButton::new(mtm);
    close.setBordered(false);
    close.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("xmark"), Some(&NSString::from_str(strings::CLOSE_TAB))).as_deref());
    close.setToolTip(Some(&NSString::from_str(strings::CLOSE_TAB)));
    close.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    close.setIdentifier(Some(&NSString::from_str(CLOSE_ID)));
    close.setHidden(true);
    for view in [&*image as &NSView, &name, &count, &close] {
        view.setTranslatesAutoresizingMaskIntoConstraints(false);
        cell.addSubview(view);
    }
    let constraints = [
        image.leadingAnchor().constraintEqualToAnchor_constant(&cell.leadingAnchor(), 2.0),
        image.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        image.widthAnchor().constraintEqualToConstant(18.0),
        name.leadingAnchor().constraintEqualToAnchor_constant(&image.trailingAnchor(), 6.0),
        name.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        count.trailingAnchor().constraintEqualToAnchor_constant(&cell.trailingAnchor(), -6.0),
        count.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        name.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&count.leadingAnchor(), -6.0),
        close.trailingAnchor().constraintEqualToAnchor_constant(&cell.trailingAnchor(), -4.0),
        close.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        close.widthAnchor().constraintEqualToConstant(16.0),
        name.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&close.leadingAnchor(), -4.0),
    ];
    for c in &constraints {
        c.setActive(true);
    }
    unsafe {
        cell.setImageView(Some(&image));
        cell.setTextField(Some(&name));
    }
    Retained::into_super(cell)
}

pub struct CellIvars {
    /// A tab's row: its cross shows with the pointer over it.
    closable: Cell<bool>,
    /// The tracking area the cell made, replaced as the cell moves.
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    /// A sidebar row that knows when the pointer is over it.
    #[unsafe(super(NSTableCellView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSidebarCell"]
    #[ivars = CellIvars]
    pub struct SidebarCell;

    impl SidebarCell {
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            if let Some(old) = self.ivars().tracking.borrow_mut().take() {
                self.removeTrackingArea(&old);
            }
            let options = NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::ActiveInActiveApp | NSTrackingAreaOptions::InVisibleRect;
            // SAFETY: the cell owns the area and removes it before making another.
            let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(<NSTrackingArea as objc2::AnyThread>::alloc(), NSZeroRect, options, Some(self), None) };
            self.addTrackingArea(&area);
            *self.ivars().tracking.borrow_mut() = Some(area);
            // SAFETY: the superclass's own handling.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
            // Rows move under a still pointer as tabs close.
            self.show_cross_if_pointed();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.show_cross(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.show_cross(false);
        }
    }
);

impl SidebarCell {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(CellIvars { closable: Cell::new(false), tracking: RefCell::new(None) });
        // SAFETY: NSView's initializer.
        unsafe { msg_send![super(this), init] }
    }

    /// Whether the row is a tab's (probe mode).
    pub fn is_closable(&self) -> bool {
        self.ivars().closable.get()
    }

    fn show_cross(&self, pointed: bool) {
        if let Some(close) = close_button(self) {
            close.setHidden(!(pointed && self.ivars().closable.get()));
        }
    }

    /// Shows the cross when the pointer is over the row now.
    fn show_cross_if_pointed(&self) {
        let inside = self.window().is_some_and(|w| {
            let at = self.convertPoint_fromView(w.mouseLocationOutsideOfEventStream(), None);
            let b = self.bounds();
            at.x >= b.origin.x && at.x < b.origin.x + b.size.width && at.y >= b.origin.y && at.y < b.origin.y + b.size.height
        });
        self.show_cross(inside);
    }
}

/// A cell's cross for closing a tab.
pub fn close_button(cell: &NSTableCellView) -> Option<Retained<NSButton>> {
    let id = NSString::from_str(CLOSE_ID);
    cell.subviews().iter().find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id))).and_then(|v| v.downcast::<NSButton>().ok())
}

pub fn count_field(cell: &NSTableCellView) -> Option<Retained<NSTextField>> {
    let id = NSString::from_str(COUNT_ID);
    cell.subviews()
        .iter()
        .find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id)))
        .and_then(|v| v.downcast::<NSTextField>().ok())
}
