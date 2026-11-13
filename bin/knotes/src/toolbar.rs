//! Window toolbar as in Knotes v1: sidebar toggle and the folders/tags switch, list title (window title and subtitle),
//! new note and back/forward over the note, the AI/outline switch and the panel's toggle over
//! the inspector (ADR 0023). Separators track
//! the split view columns, so each group sits above its column; macOS draws the glass.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSMenu, NSMenuToolbarItem, NSSearchField, NSSegmentedControl, NSSplitView, NSToolbar, NSToolbarDelegate, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarInspectorTrackingSeparatorItemIdentifier,
    NSToolbarItem, NSToolbarItemGroup, NSToolbarItemGroupSelectionMode, NSToolbarSidebarTrackingSeparatorItemIdentifier,
    NSToolbarToggleSidebarItemIdentifier, NSTrackingSeparatorToolbarItem,
};
use objc2_foundation::{NSArray, NSCopying, NSObject, NSObjectProtocol, NSString, ns_string};
use objc2_app_kit::{NSControl, NSControlTextEditingDelegate, NSEvent, NSFont, NSLineBreakMode, NSResponder, NSTextField, NSTextFieldDelegate, NSTextView, NSView};
use objc2_foundation::NSRange;

use knotes_app::{PanelMode, SearchMode, SidebarMode};

use crate::strings;

pub const SIDEBAR_MODE: &str = "knotes.sidebar-mode";
pub const LIST_SEPARATOR: &str = "knotes.list-separator";
pub const SEARCH: &str = "knotes.search";
pub const LIST_OPTIONS: &str = "knotes.list-options";
pub const NEW_NOTE: &str = "knotes.new-note";
pub const HISTORY: &str = "knotes.history";
/// The open note's name, edited in place to rename it (ADR 0019).
pub const NOTE_TITLE: &str = "knotes.note-title";
/// The AI/outline switch over the right panel (ADR 0023).
pub const PANEL_MODE: &str = "knotes.panel-mode";
/// Shows and hides the right panel.
pub const PANEL: &str = "knotes.panel";

type OnNavigate = Box<dyn Fn(bool)>;
type OnNewNote = Box<dyn Fn()>;
type OnSidebarMode = Box<dyn Fn(SidebarMode)>;
type OnPanelMode = Box<dyn Fn(PanelMode)>;
type OnSearch = Box<dyn Fn(String)>;
type OnToggleSearch = Box<dyn Fn()>;
type OnSearchMode = Box<dyn Fn(SearchMode)>;

pub struct Ivars {
    split_view: Retained<NSSplitView>,
    list_menu: Retained<NSMenu>,
    /// Called with `true` for forward, `false` for back.
    on_navigate: RefCell<Option<OnNavigate>>,
    on_new_note: RefCell<Option<OnNewNote>>,
    on_sidebar_mode: RefCell<Option<OnSidebarMode>>,
    on_search: RefCell<Option<OnSearch>>,
    on_toggle_search: RefCell<Option<OnToggleSearch>>,
    on_search_mode: RefCell<Option<OnSearchMode>>,
    /// Called with the name typed over the open note's.
    on_note_title: RefCell<Option<OnSearch>>,
    /// The open note's name field, once the toolbar has made it, and the name it shows.
    note_title: RefCell<Option<Retained<TitleField>>>,
    note_title_state: RefCell<String>,
    /// Whether the name shown can be edited (a note's).
    note_title_renamable: Cell<bool>,
    /// The folders/tags switch, once the toolbar has made it, and the mode it shows.
    sidebar_mode: RefCell<Option<Retained<NSToolbarItemGroup>>>,
    sidebar_mode_state: Cell<SidebarMode>,
    /// The switch shows only while the sidebar does.
    sidebar_shown: Cell<bool>,
    /// The AI/outline switch, once made, the mode it shows, and whether the panel shows.
    panel_mode: RefCell<Option<Retained<NSToolbarItemGroup>>>,
    panel_mode_state: Cell<PanelMode>,
    panel_shown: Cell<bool>,
    on_panel_mode: RefCell<Option<OnPanelMode>>,
    history: RefCell<Option<Retained<NSToolbarItemGroup>>>,
    /// Back/forward availability, applied when the group item is created later.
    history_state: Cell<(bool, bool)>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesToolbarDelegate"]
    #[ivars = Ivars]
    pub struct ToolbarDelegate;

    unsafe impl NSObjectProtocol for ToolbarDelegate {}

    unsafe impl NSControlTextEditingDelegate for ToolbarDelegate {
        /// Escape gives the name back.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, control: &NSControl, _view: &NSTextView, command: Sel) -> bool {
            let cancel = command == sel!(cancelOperation:);
            if cancel {
                control.setStringValue(&NSString::from_str(&self.ivars().note_title_state.borrow()));
                if let Some(window) = control.window() {
                    window.makeFirstResponder(None);
                }
                end_editing(control);
            }
            cancel
        }
    }

    unsafe impl NSTextFieldDelegate for ToolbarDelegate {}

    unsafe impl NSToolbarDelegate for ToolbarDelegate {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn item_for_identifier(&self, _toolbar: &NSToolbar, id: &NSString, _inserted: bool) -> Option<Retained<NSToolbarItem>> {
            self.make_item(id)
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn default_identifiers(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            identifiers()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn allowed_identifiers(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            identifiers()
        }
    }

    impl ToolbarDelegate {
        #[unsafe(method(newNote:))]
        fn new_note(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_new_note.borrow().as_ref() {
                callback();
            }
        }

        #[unsafe(method(sidebarMode:))]
        fn sidebar_mode_chosen(&self, sender: &NSToolbarItemGroup) {
            let mode = if sender.selectedIndex() == 1 { SidebarMode::Tags } else { SidebarMode::Folders };
            self.ivars().sidebar_mode_state.set(mode);
            if let Some(callback) = self.ivars().on_sidebar_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        #[unsafe(method(panelMode:))]
        fn panel_mode_chosen(&self, sender: &NSToolbarItemGroup) {
            let mode = if sender.selectedIndex() == 1 { PanelMode::Outline } else { PanelMode::Ai };
            self.ivars().panel_mode_state.set(mode);
            if let Some(callback) = self.ivars().on_panel_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        /// The magnifying glass: shows or hides the search row over the list.
        #[unsafe(method(toggleSearch:))]
        fn toggle_search(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_toggle_search.borrow().as_ref() {
                callback();
            }
        }

        /// The search field's text changed (it sends after a short pause, and at once on
        /// Return or the clear button).
        #[unsafe(method(searchChanged:))]
        fn search_changed(&self, sender: &NSSearchField) {
            let query = sender.stringValue().to_string();
            if let Some(callback) = self.ivars().on_search.borrow().as_ref() {
                callback(query);
            }
        }

        /// The files/content switch in the search row.
        #[unsafe(method(searchModeChanged:))]
        fn search_mode_changed(&self, sender: &NSSegmentedControl) {
            let mode = if sender.selectedSegment() == 1 { SearchMode::Content } else { SearchMode::Files };
            if let Some(callback) = self.ivars().on_search_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        /// Return (or leaving the field) renames the open note when its name changed.
        #[unsafe(method(noteTitleEdited:))]
        fn note_title_edited(&self, sender: &NSTextField) {
            let name = sender.stringValue().to_string();
            let shown = self.ivars().note_title_state.borrow().clone();
            if name.trim().is_empty() || name == shown {
                sender.setStringValue(&NSString::from_str(&shown));
            } else if let Some(callback) = self.ivars().on_note_title.borrow().as_ref() {
                callback(name);
            }
            if let Some(window) = sender.window() {
                window.makeFirstResponder(None);
            }
            end_editing(sender);
        }

        #[unsafe(method(navigate:))]
        fn navigate(&self, sender: &NSToolbarItemGroup) {
            let forward = sender.selectedIndex() == 1;
            if let Some(callback) = self.ivars().on_navigate.borrow().as_ref() {
                callback(forward);
            }
        }
    }
);

impl ToolbarDelegate {
    pub fn new(mtm: MainThreadMarker, split_view: Retained<NSSplitView>, list_menu: Retained<NSMenu>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            split_view,
            list_menu,
            on_navigate: RefCell::new(None),
            on_new_note: RefCell::new(None),
            on_sidebar_mode: RefCell::new(None),
            on_search: RefCell::new(None),
            on_toggle_search: RefCell::new(None),
            on_search_mode: RefCell::new(None),
            on_note_title: RefCell::new(None),
            note_title: RefCell::new(None),
            note_title_state: RefCell::new(String::new()),
            note_title_renamable: Cell::new(false),
            sidebar_mode: RefCell::new(None),
            sidebar_mode_state: Cell::new(SidebarMode::Folders),
            sidebar_shown: Cell::new(true),
            panel_mode: RefCell::new(None),
            panel_mode_state: Cell::new(PanelMode::Ai),
            panel_shown: Cell::new(true),
            on_panel_mode: RefCell::new(None),
            history: RefCell::new(None),
            history_state: Cell::new((false, false)),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub fn on_new_note(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_new_note.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_toggle_search(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_toggle_search.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_search_mode(&self, callback: impl Fn(SearchMode) + 'static) {
        *self.ivars().on_search_mode.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_search(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_search.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_sidebar_mode(&self, callback: impl Fn(SidebarMode) + 'static) {
        *self.ivars().on_sidebar_mode.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `mode` in the folders/tags switch (without calling back).
    pub fn set_sidebar_mode(&self, mode: SidebarMode) {
        self.ivars().sidebar_mode_state.set(mode);
        if let Some(group) = self.ivars().sidebar_mode.borrow().as_ref() {
            group.setSelectedIndex(mode_index(mode));
        }
    }

    /// Hides the folders/tags switch while the sidebar is collapsed.
    pub fn set_sidebar_shown(&self, shown: bool) {
        self.ivars().sidebar_shown.set(shown);
        if let Some(group) = self.ivars().sidebar_mode.borrow().as_ref() {
            group.setHidden(!shown);
        }
    }

    pub fn on_panel_mode(&self, callback: impl Fn(PanelMode) + 'static) {
        *self.ivars().on_panel_mode.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `mode` in the AI/outline switch (without calling back).
    pub fn set_panel_mode(&self, mode: PanelMode) {
        self.ivars().panel_mode_state.set(mode);
        if let Some(group) = self.ivars().panel_mode.borrow().as_ref() {
            group.setSelectedIndex(panel_index(mode));
        }
    }

    /// Hides the AI/outline switch while the panel is collapsed.
    pub fn set_panel_shown(&self, shown: bool) {
        self.ivars().panel_shown.set(shown);
        if let Some(group) = self.ivars().panel_mode.borrow().as_ref() {
            group.setHidden(!shown);
        }
    }

    pub fn on_navigate(&self, callback: impl Fn(bool) + 'static) {
        *self.ivars().on_navigate.borrow_mut() = Some(Box::new(callback));
    }

    /// Enables the back and forward segments.
    pub fn on_note_title(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_note_title.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows the name of the document open, editable to rename it when `renamable` (a note),
    /// unless it is being edited.
    pub fn set_note_title(&self, name: &str, renamable: bool) {
        *self.ivars().note_title_state.borrow_mut() = name.to_owned();
        self.ivars().note_title_renamable.set(renamable);
        if let Some(field) = self.ivars().note_title.borrow().as_ref() {
            field.ivars().set(renamable);
            if field.currentEditor().is_none() {
                field.setStringValue(&NSString::from_str(name));
            }
        }
    }

    pub fn set_history(&self, can_back: bool, can_forward: bool) {
        self.ivars().history_state.set((can_back, can_forward));
        if let Some(group) = self.ivars().history.borrow().as_ref() {
            apply_history(group, can_back, can_forward);
        }
    }

    fn make_item(&self, id: &NSString) -> Option<Retained<NSToolbarItem>> {
        let mtm = self.mtm();
        let target: &AnyObject = self.as_ref();
        match id.to_string().as_str() {
            SIDEBAR_MODE => {
                let images = NSArray::from_retained_slice(&[symbol("folder", strings::FOLDERS)?, symbol("tag", strings::TAGS)?]);
                let labels = NSArray::from_retained_slice(&[NSString::from_str(strings::FOLDERS), NSString::from_str(strings::TAGS)]);
                let group = unsafe {
                    NSToolbarItemGroup::groupWithItemIdentifier_images_selectionMode_labels_target_action(
                        id,
                        &images,
                        NSToolbarItemGroupSelectionMode::SelectOne,
                        Some(&labels),
                        Some(target),
                        Some(sel!(sidebarMode:)),
                        mtm,
                    )
                };
                for (item, tip) in group.subitems().iter().zip([strings::FOLDERS, strings::TAGS]) {
                    item.setToolTip(Some(&NSString::from_str(tip)));
                }
                group.setSelectedIndex(mode_index(self.ivars().sidebar_mode_state.get()));
                group.setHidden(!self.ivars().sidebar_shown.get());
                *self.ivars().sidebar_mode.borrow_mut() = Some(group.clone());
                Some(Retained::into_super(group))
            }
            LIST_SEPARATOR => Some(Retained::into_super(NSTrackingSeparatorToolbarItem::trackingSeparatorToolbarItemWithIdentifier_splitView_dividerIndex(
                id,
                &self.ivars().split_view,
                1,
            ))),
            SEARCH => {
                let item = button(mtm, id, "magnifyingglass", strings::SEARCH);
                unsafe {
                    item.setTarget(Some(target));
                    item.setAction(Some(sel!(toggleSearch:)));
                }
                Some(item)
            }
            LIST_OPTIONS => {
                let item = NSMenuToolbarItem::initWithItemIdentifier(NSMenuToolbarItem::alloc(mtm), id);
                item.setImage(symbol("ellipsis", strings::LIST_OPTIONS).as_deref());
                item.setLabel(&NSString::from_str(strings::LIST_OPTIONS));
                item.setToolTip(Some(&NSString::from_str(strings::LIST_OPTIONS)));
                item.setShowsIndicator(false);
                item.setMenu(&self.ivars().list_menu);
                Some(Retained::into_super(item))
            }
            NEW_NOTE => {
                let item = button(mtm, id, "square.and.pencil", strings::NEW_NOTE);
                unsafe {
                    item.setTarget(Some(target));
                    item.setAction(Some(sel!(newNote:)));
                }
                Some(item)
            }
            HISTORY => {
                let images = NSArray::from_retained_slice(&[symbol("chevron.left", strings::BACK)?, symbol("chevron.right", strings::FORWARD)?]);
                let labels = NSArray::from_retained_slice(&[NSString::from_str(strings::BACK), NSString::from_str(strings::FORWARD)]);
                let group = unsafe {
                    NSToolbarItemGroup::groupWithItemIdentifier_images_selectionMode_labels_target_action(
                        id,
                        &images,
                        NSToolbarItemGroupSelectionMode::Momentary,
                        Some(&labels),
                        Some(target),
                        Some(sel!(navigate:)),
                        mtm,
                    )
                };
                let (can_back, can_forward) = self.ivars().history_state.get();
                apply_history(&group, can_back, can_forward);
                *self.ivars().history.borrow_mut() = Some(group.clone());
                Some(Retained::into_super(group))
            }
            NOTE_TITLE => {
                let field = TitleField::new(mtm, self.ivars().note_title_renamable.get());
                field.setBezeled(false);
                field.setDrawsBackground(false);
                field.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize())));
                field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
                if let Some(cell) = field.cell() {
                    cell.setScrollable(true);
                    cell.setUsesSingleLineMode(true);
                    cell.setSendsActionOnEndEditing(true);
                }
                let name = self.ivars().note_title_state.borrow().clone();
                field.setStringValue(&NSString::from_str(&name));
                field.setToolTip(Some(&NSString::from_str(strings::NOTE_TITLE)));
                field.widthAnchor().constraintGreaterThanOrEqualToConstant(60.0).setActive(true);
                field.widthAnchor().constraintLessThanOrEqualToConstant(360.0).setActive(true);
                // SAFETY: the delegate outlives the toolbar (it is its delegate) and takes the action.
                unsafe {
                    field.setTarget(Some(target));
                    field.setAction(Some(sel!(noteTitleEdited:)));
                    field.setDelegate(Some(ProtocolObject::from_ref(self)));
                }
                let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), id);
                item.setView(Some(&field));
                item.setLabel(&NSString::from_str(strings::NOTE_TITLE));
                *self.ivars().note_title.borrow_mut() = Some(field);
                Some(item)
            }
            PANEL_MODE => {
                let images = NSArray::from_retained_slice(&[symbol("brain", strings::AI)?, symbol("list.bullet.indent", strings::OUTLINE)?]);
                let labels = NSArray::from_retained_slice(&[NSString::from_str(strings::AI), NSString::from_str(strings::OUTLINE)]);
                let group = unsafe {
                    NSToolbarItemGroup::groupWithItemIdentifier_images_selectionMode_labels_target_action(
                        id,
                        &images,
                        NSToolbarItemGroupSelectionMode::SelectOne,
                        Some(&labels),
                        Some(target),
                        Some(sel!(panelMode:)),
                        mtm,
                    )
                };
                for (item, tip) in group.subitems().iter().zip([strings::AI, strings::OUTLINE]) {
                    item.setToolTip(Some(&NSString::from_str(tip)));
                }
                group.setSelectedIndex(panel_index(self.ivars().panel_mode_state.get()));
                group.setHidden(!self.ivars().panel_shown.get());
                *self.ivars().panel_mode.borrow_mut() = Some(group.clone());
                Some(Retained::into_super(group))
            }
            PANEL => {
                let item = button(mtm, id, "sidebar.right", strings::TOGGLE_PANEL);
                // Up the responder chain to the split view controller.
                unsafe { item.setAction(Some(sel!(toggleInspector:))) };
                Some(item)
            }
            _ => None,
        }
    }
}

define_class!(
    /// The name of the document open, renamed in place after a double click (a single one
    /// edits nothing: it was easy to start renaming by accident). Its ivar: whether the
    /// document can be renamed.
    #[unsafe(super(NSTextField, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesTitleField"]
    #[ivars = Cell<bool>]
    pub struct TitleField;

    impl TitleField {
        /// Its clicks are its own: a label in the toolbar lets the window take them to drag
        /// it, and a double click zoomed the window instead of starting to rename.
        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() == 2 && self.ivars().get() && self.currentEditor().is_none() {
                self.begin_editing();
            } else {
                // SAFETY: forwarding the same event to the superclass implementation.
                let _: () = unsafe { msg_send![super(self), mouseDown: event] };
            }
        }
    }
);

impl TitleField {
    fn new(mtm: MainThreadMarker, renamable: bool) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(renamable));
        // SAFETY: NSTextField's initializer.
        let field: Retained<Self> = unsafe { msg_send![super(this), init] };
        end_editing(&field);
        field
    }

    /// Lets the name be edited, selected without a PDF's `.pdf`, as Finder selects a name.
    pub fn begin_editing(&self) {
        self.setEditable(true);
        self.setSelectable(true);
        if let Some(window) = self.window() {
            window.makeFirstResponder(Some(self));
        }
        let name = self.stringValue().to_string();
        let stem = name.len() - if name.to_lowercase().ends_with(".pdf") { 4 } else { 0 };
        if let Some(editor) = self.currentEditor() {
            editor.setSelectedRange(NSRange::new(0, name[..stem].encode_utf16().count()));
        }
    }
}

/// The name field is a label again once editing ends.
fn end_editing(field: &NSControl) {
    if let Some(field) = field.downcast_ref::<NSTextField>() {
        field.setEditable(false);
        field.setSelectable(false);
    }
}

pub fn identifiers() -> Retained<NSArray<NSString>> {
    // SAFETY: AppKit's identifier constants are immutable strings for the process lifetime.
    let system = unsafe {
        [
            NSToolbarToggleSidebarItemIdentifier,
            NSToolbarSidebarTrackingSeparatorItemIdentifier,
            NSToolbarFlexibleSpaceItemIdentifier,
            NSToolbarInspectorTrackingSeparatorItemIdentifier,
        ]
    };
    let custom = |s: &str| NSString::from_str(s);
    NSArray::from_retained_slice(&[
        system[0].copy(),
        custom(SIDEBAR_MODE),
        system[1].copy(),
        // No flexible space before the list options: the title already takes the free width,
        // and with the sidebar collapsed every point is needed for the separator to stay on
        // the list/note boundary.
        custom(SEARCH),
        custom(LIST_OPTIONS),
        custom(LIST_SEPARATOR),
        custom(NEW_NOTE),
        custom(HISTORY),
        custom(NOTE_TITLE),
        system[2].copy(),
        system[3].copy(),
        // The panel's switch and button at its right edge.
        system[2].copy(),
        custom(PANEL_MODE),
        custom(PANEL),
    ])
}

pub fn install(window: &objc2_app_kit::NSWindow, delegate: &ToolbarDelegate) {
    let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(window.mtm()), ns_string!("KnotesMainToolbar"));
    toolbar.setDelegate(Some(ProtocolObject::from_ref(delegate)));
    toolbar.setDisplayMode(objc2_app_kit::NSToolbarDisplayMode::IconOnly);
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(objc2_app_kit::NSWindowToolbarStyle::Unified);
}

fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), Some(&NSString::from_str(description)))
}

fn button(mtm: MainThreadMarker, id: &NSString, symbol_name: &str, label: &str) -> Retained<NSToolbarItem> {
    let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), id);
    item.setImage(symbol(symbol_name, label).as_deref());
    item.setLabel(&NSString::from_str(label));
    item.setToolTip(Some(&NSString::from_str(label)));
    item
}

fn mode_index(mode: SidebarMode) -> isize {
    match mode {
        SidebarMode::Folders => 0,
        SidebarMode::Tags => 1,
    }
}

fn panel_index(mode: PanelMode) -> isize {
    match mode {
        PanelMode::Ai => 0,
        PanelMode::Outline => 1,
    }
}

fn apply_history(group: &NSToolbarItemGroup, can_back: bool, can_forward: bool) {
    group.setAutovalidates(false);
    for (item, on) in group.subitems().iter().zip([can_back, can_forward]) {
        item.setAutovalidates(false);
        item.setEnabled(on);
    }
}
