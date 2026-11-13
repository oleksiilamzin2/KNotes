//! Sidebar: an `NSOutlineView` (source list style) over the `knotes_app::Sidebar` model. A
//! note dragged from the list onto a folder moves there, and a folder is renamed in its row
//! from the context menu (ADR 0019).

use std::cell::RefCell;
use std::collections::HashMap;

use knotes_app::{Sidebar, SidebarItem};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSControl, NSControlTextEditingDelegate, NSDragOperation, NSMenu, NSMenuItem, NSMenuItemValidation, NSTextFieldDelegate, NSTextView, NSDraggingInfo, NSFont, NSImage, NSImageView, NSOutlineView, NSOutlineViewDataSource,
    NSOutlineViewDelegate, NSPasteboard, NSScrollView, NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextField, NSUserInterfaceItemIdentification,
    NSView,
};
use objc2_foundation::{NSArray, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::note_list_view::NOTE_PATH_TYPE;
use crate::strings;

type OnSelect = Box<dyn Fn(SidebarItem)>;
type OnDrop = Box<dyn Fn(String, String)>;

pub struct Ivars {
    model: RefCell<Sidebar>,
    /// Called with the newly selected item.
    on_select: RefCell<Option<OnSelect>>,
    /// Called with the path of a note dropped on a folder, and the folder (ADR 0019).
    on_drop: RefCell<Option<OnDrop>>,
    /// Called with a folder's path and the name typed in its row.
    on_rename: RefCell<Option<OnDrop>>,
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
        /// Only folders are renamed here.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, _item: &NSMenuItem) -> bool {
            self.menu_folder().is_some()
        }
    }

    impl SidebarSource {
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
        Some(Retained::into_super(cell))
    }

    pub fn on_select(&self, callback: impl Fn(SidebarItem) + 'static) {
        *self.ivars().on_select.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_drop(&self, callback: impl Fn(String, String) + 'static) {
        *self.ivars().on_drop.borrow_mut() = Some(Box::new(callback));
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
    let menu = NSMenu::new(mtm);
    // SAFETY: the source outlives the outline (it is its data source) and implements the action.
    let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(strings::RENAME_NOTE), Some(sel!(renameFolder:)), ns_string!("")) };
    unsafe { item.setTarget(Some(source)) };
    menu.addItem(&item);
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

/// A reused cell (icon, name, count on the right), laid out with constraints.
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
    let name = NSTextField::labelWithString(ns_string!(""), mtm);
    let count = NSTextField::labelWithString(ns_string!(""), mtm);
    count.setIdentifier(Some(&NSString::from_str(COUNT_ID)));
    count.setTextColor(Some(&NSColor::secondaryLabelColor()));
    count.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(NSFont::systemFontSize(), 0.0)));
    for view in [&*image as &NSView, &name, &count] {
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
    ];
    for c in &constraints {
        c.setActive(true);
    }
    unsafe {
        cell.setImageView(Some(&image));
        cell.setTextField(Some(&name));
    }
    cell
}

pub fn count_field(cell: &NSTableCellView) -> Option<Retained<NSTextField>> {
    let id = NSString::from_str(COUNT_ID);
    cell.subviews()
        .iter()
        .find(|v| v.identifier().is_some_and(|i| i.isEqualToString(&id)))
        .and_then(|v| v.downcast::<NSTextField>().ok())
}
