//! Sidebar: an `NSOutlineView` (source list style) over the `knotes_app::Sidebar` model.

use std::cell::RefCell;
use std::collections::HashMap;

use knotes_app::{Sidebar, SidebarItem};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSColor, NSControlTextEditingDelegate, NSFont, NSImage, NSImageView, NSOutlineView, NSOutlineViewDataSource, NSOutlineViewDelegate, NSScrollView,
    NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextField, NSUserInterfaceItemIdentification, NSView,
};
use objc2_foundation::{NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::strings;

type OnSelect = Box<dyn Fn(SidebarItem)>;

pub struct Ivars {
    model: RefCell<Sidebar>,
    /// Called with the newly selected item.
    on_select: RefCell<Option<OnSelect>>,
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
    }

    unsafe impl NSControlTextEditingDelegate for SidebarSource {}

    unsafe impl NSOutlineViewDelegate for SidebarSource {
        #[unsafe(method_id(outlineView:viewForTableColumn:item:))]
        fn view_for_item(&self, outline: &NSOutlineView, _column: Option<&NSTableColumn>, item: &AnyObject) -> Option<Retained<NSView>> {
            self.cell_for(outline, item)
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
            objects: RefCell::new(HashMap::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

fn cell_for(&self, outline: &NSOutlineView, item: &AnyObject) -> Option<Retained<NSView>> {
        let item = item_of(item)?;
        let model = self.ivars().model.borrow();
        let (label, icon) = match &item {
            SidebarItem::AllNotes => (strings::ALL_NOTES.to_owned(), Some("tray.full")),
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

    pub fn set_model(&self, model: Sidebar) {
        *self.ivars().model.borrow_mut() = model;
    }

    pub fn object_for(&self, item: &SidebarItem) -> Retained<AnyObject> {
        let mut objects = self.ivars().objects.borrow_mut();
        let object = objects.entry(item.key()).or_insert_with(|| NSString::from_str(&item.key()));
        Retained::into_super(Retained::into_super(object.clone()))
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
    let scroll = NSScrollView::new(mtm);
    scroll.setDocumentView(Some(&outline));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    (scroll, outline)
}

const CELL_ID: &str = "SidebarCell";
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
