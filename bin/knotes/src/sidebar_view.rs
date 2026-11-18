//! Sidebar: an `NSOutlineView` (source list style) over the `knotes_app::Sidebar` model. A
//! note dragged from the list onto a folder moves there, and a folder is renamed in its row
//! from the context menu (ADR 0019).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use knotes_app::{Sidebar, SidebarItem, folder_space_mark};
use knotes_spaces::{KeptFromAi, Spaces};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSControl, NSControlTextEditingDelegate, NSDragOperation, NSMenu, NSMenuDelegate, NSMenuItem, NSMenuItemValidation, NSTextFieldDelegate, NSTextView, NSDraggingInfo, NSFont, NSImage, NSImageView, NSOutlineView, NSOutlineViewDataSource,
    NSOutlineViewDelegate, NSPasteboard, NSScrollView, NSTableCellView, NSTableColumn, NSTableViewStyle, NSTextField, NSUserInterfaceItemIdentification,
    NSView,
};
use objc2_foundation::{NSArray, NSEdgeInsets, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::note_list_view::NOTE_PATH_TYPE;
use crate::strings;

type OnSelect = Box<dyn Fn(SidebarItem)>;
type OnDrop = Box<dyn Fn(String, String)>;
type OnFolderSpace = Box<dyn Fn(String, FolderChoice)>;
type OnNewFolder = Box<dyn Fn(String)>;
type OnRenameEnd = Box<dyn Fn()>;
type OnDeleteFolder = Box<dyn Fn(String)>;

/// A space chosen from a folder's menu (ADR 0030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderChoice {
    Space(String),
    NoSpace,
    /// Every space (ADR 0030).
    EverySpace,
    /// A new space, named in a dialog.
    NewSpace,
}

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
    /// The vault's spaces and the window's (for a folder's menu and the rows' dots), and what
    /// choosing a space in a folder's menu calls.
    spaces: RefCell<(Arc<Spaces>, Option<String>)>,
    on_folder_space: RefCell<Option<OnFolderSpace>>,
    /// Called with the folder a new folder is made in (`""`: the vault's root).
    on_new_folder: RefCell<Option<OnNewFolder>>,
    /// Called when editing a folder's name ends, renamed or not.
    on_rename_end: RefCell<Option<OnRenameEnd>>,
    /// Called with a folder to move into the recently deleted (ADR 0032).
    on_delete_folder: RefCell<Option<OnDeleteFolder>>,
    /// What is kept from the AI, for its marks; and what a folder's menu calls to keep the
    /// folder from it or show it (ADR 0036).
    ai: RefCell<Arc<KeptFromAi>>,
    on_folder_ai: RefCell<Option<OnDeleteFolder>>,
    /// What a deck's menu calls for its settings.
    on_deck_settings: RefCell<Option<OnDeleteFolder>>,
    on_deck_table: RefCell<Option<OnDeleteFolder>>,
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
        /// A folder's menu: a space chosen for it.
        #[unsafe(method(chooseFolderSpace:))]
        fn choose_folder_space(&self, item: &NSMenuItem) {
            let Some(spec) = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string()) else { return };
            let Some((choice, folder)) = spec.split_once('\u{1}') else { return };
            let choice = match choice {
                "none" => FolderChoice::NoSpace,
                "shared" => FolderChoice::EverySpace,
                "new" => FolderChoice::NewSpace,
                id => FolderChoice::Space(id.strip_prefix("s:").unwrap_or(id).to_owned()),
            };
            if let Some(callback) = self.ivars().on_folder_space.borrow().as_ref() {
                callback(folder.to_owned(), choice);
            }
        }

        /// A folder's menu, or the sidebar's below its rows: a new folder in it, or at the top.
        #[unsafe(method(newFolder:))]
        fn new_folder(&self, item: &NSMenuItem) {
            let parent = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).and_then(|k| SidebarItem::from_key(&k.to_string())).and_then(|i| drop_folder(&i).map(str::to_owned)).unwrap_or_default();
            if let Some(callback) = self.ivars().on_new_folder.borrow().as_ref() {
                callback(parent);
            }
        }

        /// A folder's menu: the folder into the recently deleted.
        #[unsafe(method(deleteFolder:))]
        fn delete_folder(&self, item: &NSMenuItem) {
            let folder = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).and_then(|k| SidebarItem::from_key(&k.to_string())).and_then(|i| drop_folder(&i).map(str::to_owned));
            if let (Some(folder), Some(callback)) = (folder, self.ivars().on_delete_folder.borrow().as_ref()) {
                callback(folder);
            }
        }

        /// A folder's menu: kept from the AI, or shown to it again (ADR 0036).
        #[unsafe(method(toggleFolderAi:))]
        fn toggle_folder_ai(&self, item: &NSMenuItem) {
            let folder = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).and_then(|k| SidebarItem::from_key(&k.to_string())).and_then(|i| drop_folder(&i).map(str::to_owned));
            if let (Some(folder), Some(callback)) = (folder, self.ivars().on_folder_ai.borrow().as_ref()) {
                callback(folder);
            }
        }

        /// A deck's menu: its cards as a table (ADR 0037).
        #[unsafe(method(deckTable:))]
        fn deck_table(&self, item: &NSMenuItem) {
            let deck = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).and_then(|k| SidebarItem::from_key(&k.to_string()));
            if let (Some(SidebarItem::Deck(deck)), Some(callback)) = (deck, self.ivars().on_deck_table.borrow().as_ref()) {
                callback(deck);
            }
        }

        /// A deck's menu: its settings (ADR 0037).
        #[unsafe(method(deckSettings:))]
        fn deck_settings(&self, item: &NSMenuItem) {
            let deck = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).and_then(|k| SidebarItem::from_key(&k.to_string()));
            if let (Some(SidebarItem::Deck(deck)), Some(callback)) = (deck, self.ivars().on_deck_settings.borrow().as_ref()) {
                callback(deck);
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
            if matches!(item_of(item), Some(SidebarItem::Gap | SidebarItem::RestGap | SidebarItem::DeletedGap)) { GAP_HEIGHT } else { outline.rowHeight() }
        }

        #[unsafe(method(outlineView:shouldSelectItem:))]
        fn should_select(&self, _outline: &NSOutlineView, item: &AnyObject) -> bool {
            !matches!(item_of(item), Some(SidebarItem::Gap | SidebarItem::RestGap | SidebarItem::DeletedGap))
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
            spaces: RefCell::new((Arc::new(Spaces::default()), None)),
            on_folder_space: RefCell::new(None),
            on_new_folder: RefCell::new(None),
            on_rename_end: RefCell::new(None),
            on_delete_folder: RefCell::new(None),
            ai: RefCell::new(Arc::default()),
            on_folder_ai: RefCell::new(None),
            on_deck_settings: RefCell::new(None),
            on_deck_table: RefCell::new(None),
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
            SidebarItem::Gap | SidebarItem::RestGap => return Some(NSView::new(self.mtm())),
            SidebarItem::AllNotes => (strings::ALL_NOTES.to_owned(), Some("tray.full")),
            SidebarItem::Untagged => (strings::UNTAGGED.to_owned(), Some("tag.slash")),
            SidebarItem::Folder(_) if model.inbox() == Some(&item) => (model.name(&item).to_owned(), Some("tray.and.arrow.down")),
            SidebarItem::Folder(_) => (model.name(&item).to_owned(), Some("folder")),
            SidebarItem::Tag(_) => (model.name(&item).to_owned(), Some("number")),
            SidebarItem::Deck(_) => (model.name(&item).to_owned(), Some("rectangle.stack")),
            // A deck's review is a tab too (ADR 0037).
            SidebarItem::Rest => (strings::REST_OF_VAULT.to_owned(), Some("rectangle.stack")),
            SidebarItem::Deleted => (strings::RECENTLY_DELETED.to_owned(), Some("trash")),
            SidebarItem::DeletedGap => return Some(NSView::new(self.mtm())),
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
        if let Some(dot) = crate::ai_mark::dot(&cell, DOT_ID) {
            let mark = self.mark(&item);
            if let Some(space) = &mark {
                dot.setImage(crate::space_ui::dot(space.color, 9.0, &space.name).as_deref());
            }
            dot.setHidden(mark.is_none());
        }
        // The AI's mark: a folder it never looks into, a tab's document it never sees (ADR 0036).
        crate::ai_mark::show(&cell, self.kept_from_ai(&item));
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

    /// Fills the context menu for `item`'s row: a folder's rename, a tab's closing, nothing
    /// for other rows. Each item knows its row's item.
    pub fn fill_menu(&self, menu: &NSMenu, item: Option<&SidebarItem>) {
        menu.removeAllItems();
        let ai = if item.is_some_and(|i| self.kept_from_ai(i)) { strings::SHOW_TO_AI } else { strings::KEEP_FROM_AI };
        let entries: &[(&str, Sel)] = match item {
            Some(SidebarItem::Folder(_)) => &[(strings::RENAME_NOTE, sel!(renameFolder:)), (strings::NEW_SUBFOLDER, sel!(newFolder:)), (ai, sel!(toggleFolderAi:)), (strings::DELETE, sel!(deleteFolder:))],
            // All the decks together have no settings of their own: each deck's are its own.
            Some(SidebarItem::Deck(deck)) if deck.is_empty() => &[(strings::DECK_TABLE, sel!(deckTable:))],
            Some(SidebarItem::Deck(_)) => &[(strings::DECK_TABLE, sel!(deckTable:)), (strings::DECK_SETTINGS, sel!(deckSettings:))],
            // Below the rows: a new folder at the vault's top.
            None => &[(strings::NEW_FOLDER, sel!(newFolder:))],
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
        if let Some(SidebarItem::Folder(folder)) = item {
            let space = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm()), &NSString::from_str(strings::SPACE), None, ns_string!("")) };
            space.setSubmenu(Some(&self.folder_space_menu(folder)));
            menu.addItem(&space);
        }
    }

    /// A folder's «Простір ▸»: the spaces with their dots and none, the folder's ticked, and a
    /// new space (ADR 0030).
    pub fn folder_space_menu(&self, folder: &str) -> Retained<NSMenu> {
        let mtm = self.mtm();
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(strings::SPACE));
        let spaces = Arc::clone(&self.ivars().spaces.borrow().0);
        let current = spaces.of_folder(folder).map(|s| s.id.clone());
        let shared = spaces.shared(folder);
        let entry = |title: &str, choice: &str, ticked: bool| {
            // SAFETY: the source outlives the menu (the outline keeps it) and implements the action.
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(chooseFolderSpace:)), ns_string!("")) };
            // SAFETY: as above; the represented object is a string.
            unsafe {
                item.setTarget(Some(self));
                item.setRepresentedObject(Some(NSString::from_str(&format!("{choice}\u{1}{folder}")).as_ref()));
            }
            item.setState(if ticked { objc2_app_kit::NSControlStateValueOn } else { objc2_app_kit::NSControlStateValueOff });
            item
        };
        for space in spaces.list() {
            let item = entry(&space.name, &format!("s:{}", space.id), current.as_deref() == Some(space.id.as_str()));
            item.setImage(crate::space_ui::dot(space.color, 11.0, &space.name).as_deref());
            menu.addItem(&item);
        }
        menu.addItem(&entry(strings::NO_SPACE, "none", current.is_none() && !shared));
        menu.addItem(&entry(strings::EVERY_SPACE, "shared", shared));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&entry(strings::NEW_SPACE, "new", false));
        menu
    }

    pub fn set_spaces(&self, spaces: Arc<Spaces>, current: Option<String>) {
        *self.ivars().spaces.borrow_mut() = (spaces, current);
    }

    /// What is kept from the AI, whose folders and tabs the rows mark (ADR 0036).
    pub fn set_ai(&self, ai: Arc<KeptFromAi>) {
        *self.ivars().ai.borrow_mut() = ai;
    }

    /// Called with a deck whose menu asks its settings.
    pub fn on_deck_table(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_deck_table.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_deck_settings(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_deck_settings.borrow_mut() = Some(Box::new(callback));
    }

    /// Called with a folder its menu keeps from the AI or shows it.
    pub fn on_folder_ai(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_folder_ai.borrow_mut() = Some(Box::new(callback));
    }

    /// Whether the AI never sees `item`: a folder, or a tab's document.
    fn kept_from_ai(&self, item: &SidebarItem) -> bool {
        let ai = self.ivars().ai.borrow();
        match item {
            SidebarItem::Folder(folder) => ai.holds_folder(folder),
            _ => false,
        }
    }

    /// The rows drawn with the AI's mark, for the probe: "Diary | b" (their names).
    pub fn probe_ai_marks(&self) -> String {
        let Some(outline) = self.ivars().outline.borrow().load() else { return String::new() };
        let marked: Vec<String> = (0..outline.numberOfRows())
            .filter_map(|row| outline.viewAtColumn_row_makeIfNecessary(0, row, true))
            .filter(|cell| crate::ai_mark::shown(cell))
            // SAFETY: the cells are built by `cell` with their text field set.
            .filter_map(|cell| cell.downcast::<NSTableCellView>().ok().and_then(|cell| unsafe { cell.textField() }).map(|f| f.stringValue().to_string()))
            .collect();
        marked.join(" | ")
    }

    /// The space whose dot a row shows (ADR 0030): a folder's where it sets one, a tab's
    /// document's of another space.
    fn mark(&self, item: &SidebarItem) -> Option<knotes_spaces::Space> {
        let (spaces, current) = &*self.ivars().spaces.borrow();
        match item {
            SidebarItem::Folder(folder) => folder_space_mark(spaces, current.as_deref(), folder).cloned(),
            _ => None,
        }
    }

    /// The marked rows shown, for the probe: "Home • Дім | Work • Робота".
    pub fn probe_marks(&self) -> String {
        let Some(outline) = self.ivars().outline.borrow().load() else { return String::new() };
        let model = self.ivars().model.borrow();
        let marked: Vec<String> = (0..outline.numberOfRows())
            .filter_map(|row| outline.itemAtRow(row).and_then(|o| item_of(&o)))
            .filter_map(|item| self.mark(&item).map(|space| format!("{} • {}", model.name(&item), space.name)))
            .collect();
        marked.join(" | ")
    }

    pub fn on_new_folder(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_new_folder.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_delete_folder(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_delete_folder.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_rename_end(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_rename_end.borrow_mut() = Some(Box::new(callback));
    }

    /// Whether a folder's name is being edited (the sidebar waits to be rebuilt).
    pub fn is_renaming(&self) -> bool {
        self.ivars().renaming.borrow().is_some()
    }

    pub fn on_folder_space(&self, callback: impl Fn(String, FolderChoice) + 'static) {
        *self.ivars().on_folder_space.borrow_mut() = Some(Box::new(callback));
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
        // The folder's own name, though its row shows a path (one hanging away from its parent).
        name.setStringValue(&NSString::from_str(folder.rsplit('/').next().unwrap_or(&folder)));
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
        } else if let Some(callback) = self.ivars().on_rename.borrow().as_ref() {
            callback(folder, name);
        }
        // The sidebar, which waited while the name was edited, shows its rows again.
        if let Some(callback) = self.ivars().on_rename_end.borrow().as_ref() {
            callback();
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
    // Filled for the row clicked as it opens.
    let menu = NSMenu::new(mtm);
    menu.setDelegate(Some(ProtocolObject::from_ref(source)));
    // SAFETY: a menu built here, kept by the outline.
    unsafe { outline.setMenu(Some(&menu)) };
    // Without the line the toolbar draws over a scroll view under it (ADR 0016).
    let scroll: Retained<NSScrollView> = Retained::into_super(crate::quiet_scroll_view::QuietScrollView::new(mtm));
    scroll.setDocumentView(Some(&outline));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    // A little room above the first row (owner's request): under the row of the toolbar it
    // started right at the edge.
    scroll.setAdditionalSafeAreaInsets(NSEdgeInsets { top: TOP_ROOM, left: 0.0, bottom: 0.0, right: 0.0 });
    (scroll, outline)
}

/// Room above the first row.
const TOP_ROOM: f64 = 8.0;
const CELL_ID: &str = "SidebarCell";
/// Height of the empty row between the fixed rows and the tree.
const GAP_HEIGHT: f64 = 10.0;
const COUNT_ID: &str = "count";
/// A row's space dot (ADR 0030), among its marks.
const DOT_ID: &str = "space-dot";

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
    // The AI's mark (ADR 0036) and the dot of a space (ADR 0030), right after the name; hidden
    // on most rows.
    let dot = NSImageView::new(mtm);
    dot.setIdentifier(Some(&NSString::from_str(DOT_ID)));
    dot.setHidden(true);
    let marks = crate::ai_mark::marks(mtm, &dot);
    for view in [&*image as &NSView, &name, &count, &marks] {
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
        marks.leadingAnchor().constraintEqualToAnchor_constant(&name.trailingAnchor(), 5.0),
        marks.centerYAnchor().constraintEqualToAnchor(&cell.centerYAnchor()),
        marks.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&count.leadingAnchor(), -6.0),
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
