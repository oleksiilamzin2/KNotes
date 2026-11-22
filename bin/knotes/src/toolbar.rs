//! The window's toolbar, in two rows (ADR 0038). The toolbar itself: the sidebar's toggle at
//! the left, the panel's at the right; macOS draws their glass. Under it a plain row
//! (`second_row`), each part over its split view column: what the sidebar lists, the list's
//! name, count and search, the note's buttons, name and date, what the panel shows.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSMenu, NSSearchField, NSSegmentedControl, NSSplitView, NSToolbar, NSToolbarDelegate, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarInspectorTrackingSeparatorItemIdentifier,
    NSToolbarItem, NSToolbarSidebarTrackingSeparatorItemIdentifier,
    NSToolbarToggleSidebarItemIdentifier,
};
use objc2::AllocAnyThread;
use objc2_foundation::{NSArray, NSCopying, NSDate, NSDateFormatter, NSDateFormatterStyle, NSLocale, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, ns_string};
use objc2_app_kit::{
    NSApplication, NSColor, NSControl, NSControlTextEditingDelegate, NSEvent, NSEventType, NSFocusRingType, NSFont, NSLayoutAttribute, NSBox, NSBoxType, NSButton, NSControlStateValueOff, NSControlStateValueOn, NSLayoutConstraint, NSLayoutGuide, NSLayoutConstraintOrientation, NSLineBreakMode, NSResponder, NSSplitViewDidResizeSubviewsNotification, NSTextField, NSTextFieldDelegate, NSTextView,
    NSUserInterfaceItemIdentification, NSView, NSViewFrameDidChangeNotification,
};

use knotes_app::{PanelMode, SearchMode, SidebarMode};

use crate::strings;

/// The tabs, along the toolbar between the sidebar's toggle and the panel's (ADR 0024).
pub const TABS: &str = "knotes.tabs";
/// After the tabs, side by side: the vault's search (a window of its own), and a new note (the owner's choice over a
/// «+» for a new tab, which was there first).
pub const TOOLBAR_SEARCH: &str = "knotes.toolbar-search";
pub const TOOLBAR_NEW_NOTE: &str = "knotes.toolbar-new-note";
/// Zen mode: the note alone in the window.
pub const TOOLBAR_ZEN: &str = "knotes.toolbar-zen";
pub const SEARCH: &str = "knotes.search";
/// The list's search field, in the place of the list's name while the search shows.
pub const SEARCH_FIELD: &str = "knotes.search-field";
pub const LIST_OPTIONS: &str = "knotes.list-options";
/// Empties the recently deleted, over their list.
pub const EMPTY_DELETED: &str = "knotes.empty-deleted";
pub const HISTORY_BACK: &str = "knotes.history-back";
pub const HISTORY_FORWARD: &str = "knotes.history-forward";
/// The list's name and its count of notes, in the row under the toolbar.
pub const LIST_TITLE: &str = "knotes.list-title";
pub const LIST_COUNT: &str = "knotes.list-count";
/// When the open document was last changed, after its name.
pub const NOTE_DATE: &str = "knotes.note-date";
/// The open note's name, edited in place to rename it (ADR 0019).
pub const NOTE_TITLE: &str = "knotes.note-title";
/// The AI/outline switch over the right panel (ADR 0023).
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
    on_search_vault: RefCell<Option<OnToggleSearch>>,
    on_zen: RefCell<Option<OnToggleSearch>>,
    on_empty_deleted: RefCell<Option<OnToggleSearch>>,
    /// The recently deleted are listed and there are some: their button shows.
    empty_shown: Cell<bool>,
    /// Zen mode's button, once made: its picture says which way a click goes.
    zen: RefCell<Option<Retained<NSToolbarItem>>>,
    on_search_mode: RefCell<Option<OnSearchMode>>,
    /// Called with the name typed over the open note's.
    on_note_title: RefCell<Option<OnSearch>>,
    /// Called when editing the name ends by a key (Return or Escape): the note takes the keys.
    on_title_done: RefCell<Option<OnNewNote>>,
    /// Called when the search row's «Aa» is clicked.
    on_search_case: RefCell<Option<OnNewNote>>,
    /// The open note's name field, once the toolbar has made it, and the name it shows.
    note_title: RefCell<Option<Retained<TitleField>>>,
    note_title_state: RefCell<String>,
    /// Under the name, when the document was last changed (as the list's count under its
    /// title), and what it says.
    note_date: RefCell<Option<Retained<NSTextField>>>,
    /// The list's name and its count of notes, as the row shows them.
    list_title_state: RefCell<(String, String)>,
    note_date_state: RefCell<String>,
    /// In Ukrainian, as the rest of the window: the region settings' language may be another.
    date_format: Retained<NSDateFormatter>,
    /// Whether the name shown can be edited (a note's).
    note_title_renamable: Cell<bool>,
    /// The folders/tags switch, once the toolbar has made it, and the mode it shows.
    sidebar_mode_state: Cell<SidebarMode>,
    /// The switch shows only while the sidebar does.
    sidebar_shown: Cell<bool>,
    /// The AI/outline switch, once made, the mode it shows, and whether the panel shows.
    panel_mode_state: Cell<PanelMode>,
    panel_shown: Cell<bool>,
    on_panel_mode: RefCell<Option<OnPanelMode>>,
    /// The row under the toolbar: its parts over the list and over the note, and its controls.
    row: RefCell<Option<Row>>,
    /// The tabs' strip, the toolbar's item between its two toggles.
    tabs: RefCell<Option<Retained<NSView>>>,
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
                self.title_done();
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
        fn sidebar_mode_chosen(&self, sender: &NSButton) {
            let mode = match sender.tag() {
                1 => SidebarMode::Tags,
                2 => SidebarMode::Cards,
                _ => SidebarMode::Folders,
            };
            self.set_sidebar_mode(mode);
            if let Some(callback) = self.ivars().on_sidebar_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        #[unsafe(method(panelMode:))]
        fn panel_mode_chosen(&self, sender: &NSButton) {
            let mode = match sender.tag() {
                0 => PanelMode::Stars,
                1 => PanelMode::Outline,
                2 => PanelMode::Links,
                3 => PanelMode::Calendar,
                _ => PanelMode::Ai,
            };
            self.set_panel_mode(mode);
            if let Some(callback) = self.ivars().on_panel_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        /// The magnifying glass: shows or hides the search row over the list.
        /// The toolbar's magnifying glass: the vault's search, in a window of its own.
        #[unsafe(method(searchVault:))]
        fn search_vault(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_search_vault.borrow().as_ref() {
                callback();
            }
        }

        #[unsafe(method(emptyDeleted:))]
        fn empty_deleted(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_empty_deleted.borrow().as_ref() {
                callback();
            }
        }

        #[unsafe(method(toggleZen:))]
        fn toggle_zen(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_zen.borrow().as_ref() {
                callback();
            }
        }

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
            let mode = match sender.selectedSegment() {
                1 => SearchMode::Content,
                2 => SearchMode::Meaning,
                _ => SearchMode::Files,
            };
            if let Some(callback) = self.ivars().on_search_mode.borrow().as_ref() {
                callback(mode);
            }
        }

        /// The search row's «Aa»: the query searched again, its case ignored or kept.
        #[unsafe(method(searchCaseChanged:))]
        fn search_case_changed(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_search_case.borrow().as_ref() {
                callback();
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
            // Ended by Return, not by a click elsewhere (which keeps the focus it gave).
            let by_key = NSApplication::sharedApplication(self.mtm()).currentEvent().is_some_and(|e| e.r#type() == NSEventType::KeyDown);
            if by_key {
                self.title_done();
            }
        }

        #[unsafe(method(navigateBack:))]
        fn navigate_back(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_navigate.borrow().as_ref() {
                callback(false);
            }
        }

        #[unsafe(method(navigateForward:))]
        fn navigate_forward(&self, _sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_navigate.borrow().as_ref() {
                callback(true);
            }
        }

        /// The row's «…»: the list's menu, under the button.
        #[unsafe(method(listOptions:))]
        fn list_options(&self, sender: &NSView) {
            let below = NSPoint::new(0.0, sender.bounds().size.height + 4.0);
            self.ivars().list_menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(sender));
        }

        /// The split view's columns changed: the row's parts follow them.
        #[unsafe(method(columnsResized:))]
        fn columns_resized(&self, _notification: &NSNotification) {
            self.lay_out_row();
        }
    }
);

impl ToolbarDelegate {
    pub fn new(mtm: MainThreadMarker, split_view: Retained<NSSplitView>, list_menu: Retained<NSMenu>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            tabs: RefCell::new(None),
            split_view,
            list_menu,
            on_navigate: RefCell::new(None),
            on_new_note: RefCell::new(None),
            on_sidebar_mode: RefCell::new(None),
            on_search: RefCell::new(None),
            on_toggle_search: RefCell::new(None),
            on_search_vault: RefCell::new(None),
            on_zen: RefCell::new(None),
            on_empty_deleted: RefCell::new(None),
            empty_shown: Cell::new(false),
            zen: RefCell::new(None),
            on_search_mode: RefCell::new(None),
            on_note_title: RefCell::new(None),
            on_title_done: RefCell::new(None),
            on_search_case: RefCell::new(None),
            note_title: RefCell::new(None),
            note_title_state: RefCell::new(String::new()),
            note_date: RefCell::new(None),
            list_title_state: RefCell::new((String::new(), String::new())),
            note_date_state: RefCell::new(String::new()),
            date_format: ukrainian_dates(),
            note_title_renamable: Cell::new(false),
            sidebar_mode_state: Cell::new(SidebarMode::Folders),
            sidebar_shown: Cell::new(true),
            panel_mode_state: Cell::new(PanelMode::Ai),
            panel_shown: Cell::new(true),
            on_panel_mode: RefCell::new(None),
            row: RefCell::new(None),
            history_state: Cell::new((false, false)),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub fn on_new_note(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_new_note.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_search_vault(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_search_vault.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_empty_deleted(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_empty_deleted.borrow_mut() = Some(Box::new(callback));
    }

    /// The button emptying the recently deleted, shown over their list while it has entries.
    pub fn set_empty_shown(&self, shown: bool) {
        self.ivars().empty_shown.set(shown);
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.empty.setHidden(!shown);
        }
    }

    pub fn on_zen(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_zen.borrow_mut() = Some(Box::new(callback));
    }

    /// Zen mode: the tabs hidden, and the zen button's picture: arrows out to enter, arrows in to leave.
    pub fn set_zen(&self, on: bool) {
        // The tabs go too: the note alone.
        if let Some(tabs) = self.ivars().tabs.borrow().as_ref() {
            tabs.setHidden(on);
        }
        // And of the row below only the note's name stays.
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.back.setHidden(on);
            row.forward.setHidden(on);
            // One off before the other on: the two together cannot hold.
            let (off, now) = if on { (&row.title_left, &row.title_middle) } else { (&row.title_middle, &row.title_left) };
            off.setActive(false);
            now.setActive(true);
        }
        if let Some(date) = self.ivars().note_date.borrow().as_ref() {
            date.setHidden(on || self.ivars().note_date_state.borrow().is_empty());
        }
        if let Some(item) = self.ivars().zen.borrow().as_ref() {
            item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(zen_symbol(on)), Some(&NSString::from_str(strings::ZEN))).as_deref());
        }
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
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            choose(&row.modes, mode_index(mode));
        }
    }

    /// Hides the folders/tags switch while the sidebar is collapsed.
    pub fn set_sidebar_shown(&self, shown: bool) {
        self.ivars().sidebar_shown.set(shown);
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.over_sidebar.setHidden(!shown);
        }
    }



    pub fn on_panel_mode(&self, callback: impl Fn(PanelMode) + 'static) {
        *self.ivars().on_panel_mode.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `mode` in the AI/outline switch (without calling back).
    pub fn set_panel_mode(&self, mode: PanelMode) {
        self.ivars().panel_mode_state.set(mode);
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            choose(&row.panels, panel_index(mode));
        }
    }

    /// Hides the AI/outline switch while the panel is collapsed.
    pub fn set_panel_shown(&self, shown: bool) {
        self.ivars().panel_shown.set(shown);
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.over_panel.setHidden(!shown);
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
    /// unless it is being edited; under it, when it was last changed (seconds since 1970).
    pub fn set_note_title(&self, name: &str, renamable: bool, changed: Option<f64>) {
        let before = self.ivars().note_title_state.replace(name.to_owned());
        self.ivars().note_title_renamable.set(renamable);
        if let Some(field) = self.ivars().note_title.borrow().as_ref() {
            field.ivars().set(renamable);
            // Being edited, it keeps what is typed; untouched (a new note's «Без назви» while the
            // note is named after its first line), it follows: left as it was, the end of the
            // editing renamed the note back.
            let untouched = field.currentEditor().is_some() && field.stringValue().to_string() == before;
            if field.currentEditor().is_none() || untouched {
                field.setStringValue(&NSString::from_str(name));
            }
        }
        let date = changed.map(|seconds| self.ivars().date_format.stringFromDate(&NSDate::dateWithTimeIntervalSince1970(seconds)).to_string()).unwrap_or_default();
        *self.ivars().note_date_state.borrow_mut() = date.clone();
        if let Some(field) = self.ivars().note_date.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(&date));
            // Zen mode (the tabs hidden) shows no date.
            let zen = self.ivars().tabs.borrow().as_ref().is_some_and(|tabs| tabs.isHidden());
            field.setHidden(date.is_empty() || zen);
        }
    }


    /// Called when the search row's «Aa» is clicked.
    pub fn on_search_case(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_search_case.borrow_mut() = Some(Box::new(callback));
    }

    /// Called when editing the name ends by Return or Escape.
    pub fn on_title_done(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_title_done.borrow_mut() = Some(Box::new(callback));
    }

    fn title_done(&self) {
        if let Some(callback) = self.ivars().on_title_done.borrow().as_ref() {
            callback();
        }
    }

    /// Lets a new note's name be typed: the field empty, its «Без назви» grey in it until a
    /// letter is typed, the cursor there (no selection flashing over the name, owner's word).
    /// Left empty, the note keeps its name.
    pub fn begin_naming(&self) {
        if let Some(title) = self.ivars().note_title.borrow().as_ref() {
            title.setPlaceholderString(Some(&NSString::from_str(&self.ivars().note_title_state.borrow())));
            title.setStringValue(&NSString::new());
            title.begin_editing();
        }
    }

    pub fn set_history(&self, can_back: bool, can_forward: bool) {
        self.ivars().history_state.set((can_back, can_forward));
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.back.setEnabled(can_back);
            row.forward.setEnabled(can_forward);
        }
    }

    /// The list's name and its count of notes, over the list in the row.
    pub fn set_list_title(&self, title: &str, count: &str) {
        *self.ivars().list_title_state.borrow_mut() = (title.to_owned(), count.to_owned());
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.list_title.setStringValue(&NSString::from_str(title));
            row.list_count.setStringValue(&NSString::from_str(count));
        }
    }

    /// The list's search shown or hidden: its field (`search_field`, shown by its owner) stands
    /// in the place of the list's name and count.
    pub fn set_search_shown(&self, shown: bool) {
        if let Some(row) = self.ivars().row.borrow().as_ref() {
            row.list_title.setHidden(shown);
            row.list_count.setHidden(shown);
        }
    }

    /// The row under the toolbar (owner's request, 2026-11-17, after DEVONthink's): what belongs
    /// to a column over that column, as plain text and small buttons on the bar itself, no glass.
    /// Over the list: its name, its count, the search and its options; while the search shows,
    /// `search_field` in the place of the name and the count. Over the note: back and forward,
    /// the document's name (renamed in place) and when it was changed.
    fn second_row(&self, search_field: &NSSearchField) -> Retained<NSView> {
        let mtm = self.mtm();
        let target: &AnyObject = self.as_ref();
        let small = NSFont::smallSystemFontSize() + 1.0;
        let label = |id: &str, bold: bool| {
            let field = NSTextField::labelWithString(&NSString::new(), mtm);
            let font = if bold { NSFont::boldSystemFontOfSize(small) } else { NSFont::systemFontOfSize(small) };
            let colour = if bold { NSColor::labelColor() } else { NSColor::secondaryLabelColor() };
            field.setFont(Some(&font));
            field.setTextColor(Some(&colour));
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            field.setIdentifier(Some(&NSString::from_str(id)));
            field
        };
        let button = |id: &str, symbol_name: &str, tip: &str, action: Sel| {
            let image = symbol(symbol_name, tip).unwrap_or_else(NSImage::new);
            // SAFETY: the delegate outlives the row (the window keeps both) and takes the action.
            let button = unsafe { NSButton::buttonWithImage_target_action(&image, Some(target), Some(action), mtm) };
            button.setBordered(false);
            button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
            button.setToolTip(Some(&NSString::from_str(tip)));
            button.setIdentifier(Some(&NSString::from_str(id)));
            button
        };
        let (list_title, list_count) = (label(LIST_TITLE, true), label(LIST_COUNT, false));
        search_field.setIdentifier(Some(&NSString::from_str(SEARCH_FIELD)));
        let search = button(SEARCH, "magnifyingglass", strings::SEARCH, sel!(toggleSearch:));
        let empty = button(EMPTY_DELETED, "trash.slash", strings::EMPTY_DELETED, sel!(emptyDeleted:));
        empty.setHidden(!self.ivars().empty_shown.get());
        let options = button(LIST_OPTIONS, "ellipsis", strings::LIST_OPTIONS, sel!(listOptions:));
        let back = button(HISTORY_BACK, "chevron.left", strings::BACK, sel!(navigateBack:));
        let forward = button(HISTORY_FORWARD, "chevron.right", strings::FORWARD, sel!(navigateForward:));
        let title = TitleField::new(mtm, self.ivars().note_title_renamable.get());
        title.setBezeled(false);
        title.setDrawsBackground(false);
        // No focus ring while it is edited: it grew in as an animation, which stuttered while a new
        // note was being made (owner's report); the cursor and the grey «Без назви» say enough.
        title.setFocusRingType(NSFocusRingType::None);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(small + 1.0)));
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        title.setIdentifier(Some(&NSString::from_str(NOTE_TITLE)));
        if let Some(cell) = title.cell() {
            cell.setScrollable(true);
            cell.setUsesSingleLineMode(true);
            cell.setSendsActionOnEndEditing(true);
        }
        title.setToolTip(Some(&NSString::from_str(strings::NOTE_TITLE)));
        // SAFETY: as the buttons'.
        unsafe {
            title.setTarget(Some(target));
            title.setAction(Some(sel!(noteTitleEdited:)));
            title.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
        let date = label(NOTE_DATE, false);
        date.setToolTip(Some(&NSString::from_str(strings::NOTE_CHANGED)));
        let row = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, ROW_HEIGHT)));
        let (over_list, over_note) = (NSView::new(mtm), NSView::new(mtm));
        // The grey of the columns at the ends (the owner's sample: 237, 237, 238), behind the
        // buttons over the sidebar and over the panel: the sidebar's material was darker than the
        // panel under it. In the dark the panel's (37, 37, 39): the window's background there is
        // darker than the columns, and showed as dark plates over them.
        let grey = crate::live_preview::appearance_color(0xEDEDEE, 0x252527);
        let side = || {
            let plate = NSBox::new(mtm);
            plate.setBoxType(NSBoxType::Custom);
            plate.setBorderWidth(0.0);
            plate.setCornerRadius(0.0);
            plate.setFillColor(&grey);
            plate.setContentViewMargins(NSSize::new(0.0, 0.0));
            Retained::into_super(plate) as Retained<NSView>
        };
        // Small buttons in a row in the middle of `part`, the one chosen in the accent colour.
        let switch = |part: &NSView, buttons: &[Retained<NSButton>]| {
            // A guide as wide as the buttons with their gaps, centred: any number of them.
            let group = NSLayoutGuide::new();
            part.addLayoutGuide(&group);
            group.centerXAnchor().constraintEqualToAnchor(&part.centerXAnchor()).setActive(true);
            for (tag, button) in buttons.iter().enumerate() {
                button.setTag(tag as isize);
                button.setTranslatesAutoresizingMaskIntoConstraints(false);
                part.addSubview(button);
                button.centerYAnchor().constraintEqualToAnchor(&part.centerYAnchor()).setActive(true);
                match tag.checked_sub(1).and_then(|before| buttons.get(before)) {
                    Some(before) => button.leadingAnchor().constraintEqualToAnchor_constant(&before.trailingAnchor(), 16.0).setActive(true),
                    None => button.leadingAnchor().constraintEqualToAnchor(&group.leadingAnchor()).setActive(true),
                }
            }
            if let Some(last) = buttons.last() {
                last.trailingAnchor().constraintEqualToAnchor(&group.trailingAnchor()).setActive(true);
            }
        };
        let (over_sidebar, over_panel) = (side(), side());
        // Over the panel, what it shows: the stars first and the AI last (the owner's order),
        // the outline, the links and the calendar between them.
        let panels = [
            button(PANEL_MODE_IDS[0], "star", strings::STARS, sel!(panelMode:)),
            button(PANEL_MODE_IDS[1], "list.bullet.indent", strings::OUTLINE, sel!(panelMode:)),
            button(PANEL_MODE_IDS[2], "link", strings::CONNECTIONS, sel!(panelMode:)),
            button(PANEL_MODE_IDS[3], "calendar", strings::CALENDAR, sel!(panelMode:)),
            button(PANEL_MODE_IDS[4], "brain", strings::AI, sel!(panelMode:)),
        ];
        switch(&over_panel, &panels);
        choose(&panels, panel_index(self.ivars().panel_mode_state.get()));
        over_panel.setHidden(!self.ivars().panel_shown.get());
        row.addSubview(&over_panel);
        // Over the sidebar, what it lists: folders, tags or the cards' decks, three small buttons
        // in its middle, the one chosen in the accent colour. (The tabs were a fourth; they are
        // along the toolbar now.)
        let modes = [
            button(SIDEBAR_MODE_IDS[0], "folder", strings::FOLDERS, sel!(sidebarMode:)),
            button(SIDEBAR_MODE_IDS[1], "tag", strings::TAGS, sel!(sidebarMode:)),
            button(SIDEBAR_MODE_IDS[2], "rectangle.stack", strings::CARDS, sel!(sidebarMode:)),
        ];
        switch(&over_sidebar, &modes);
        choose(&modes, mode_index(self.ivars().sidebar_mode_state.get()));
        over_sidebar.setHidden(!self.ivars().sidebar_shown.get());
        row.addSubview(&over_sidebar);
        row.addSubview(&over_list);
        row.addSubview(&over_note);
        for (part, views) in [(&over_list, [&*list_title as &NSView, &list_count, search_field, &empty, &search, &options].as_slice()), (&over_note, [&*back as &NSView, &forward, &title, &date].as_slice())] {
            for view in views {
                view.setTranslatesAutoresizingMaskIntoConstraints(false);
                part.addSubview(view);
                view.centerYAnchor().constraintEqualToAnchor(&part.centerYAnchor()).setActive(true);
            }
        }
        // The names give way before the buttons; the count and the date before the names.
        for (field, priority) in [(&*list_title as &NSView, 300.0), (&list_count, 200.0), (&title, 300.0), (&date, 200.0)] {
            field.setContentCompressionResistancePriority_forOrientation(priority, NSLayoutConstraintOrientation::Horizontal);
        }
        let constraints = [
            list_title.leadingAnchor().constraintEqualToAnchor_constant(&over_list.leadingAnchor(), 12.0),
            list_count.leadingAnchor().constraintEqualToAnchor_constant(&list_title.trailingAnchor(), 6.0),
            list_count.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&empty.leadingAnchor(), -6.0),
            empty.trailingAnchor().constraintEqualToAnchor_constant(&search.leadingAnchor(), -10.0),
            search_field.leadingAnchor().constraintEqualToAnchor_constant(&over_list.leadingAnchor(), 12.0),
            search_field.trailingAnchor().constraintEqualToAnchor_constant(&search.leadingAnchor(), -8.0),
            search.trailingAnchor().constraintEqualToAnchor_constant(&options.leadingAnchor(), -10.0),
            options.trailingAnchor().constraintEqualToAnchor_constant(&over_list.trailingAnchor(), -12.0),
            // (A new note's button stood first here; it is in the toolbar now, after the tabs.)
            back.leadingAnchor().constraintEqualToAnchor_constant(&over_note.leadingAnchor(), 12.0),
            forward.leadingAnchor().constraintEqualToAnchor_constant(&back.trailingAnchor(), 12.0),
            date.leadingAnchor().constraintEqualToAnchor_constant(&title.trailingAnchor(), 8.0),
            date.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&over_note.trailingAnchor(), -12.0),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        // The name after the arrows; in zen mode in the middle (`set_zen`).
        let title_left = title.leadingAnchor().constraintEqualToAnchor_constant(&forward.trailingAnchor(), 12.0);
        let title_middle = title.centerXAnchor().constraintEqualToAnchor(&over_note.centerXAnchor());
        title_left.setActive(true);
        let (can_back, can_forward) = self.ivars().history_state.get();
        back.setEnabled(can_back);
        forward.setEnabled(can_forward);
        let (name, count) = self.ivars().list_title_state.borrow().clone();
        list_title.setStringValue(&NSString::from_str(&name));
        list_count.setStringValue(&NSString::from_str(&count));
        title.setStringValue(&NSString::from_str(&self.ivars().note_title_state.borrow()));
        date.setStringValue(&NSString::from_str(&self.ivars().note_date_state.borrow()));
        *self.ivars().note_title.borrow_mut() = Some(title);
        *self.ivars().note_date.borrow_mut() = Some(date);
        *self.ivars().row.borrow_mut() = Some(Row { view: row.clone(), over_sidebar, modes, over_panel, panels, over_list, over_note, list_title, list_count, empty, back, forward, title_left, title_middle });
        // SAFETY: the delegate lives as long as the window; the selector takes a notification.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(target, sel!(columnsResized:), Some(NSSplitViewDidResizeSubviewsNotification), Some(self.ivars().split_view.as_ref()));
            // And when the row itself is placed or resized (it starts where the sidebar ends, and
            // is put there after the columns are first laid out).
            row.setPostsFrameChangedNotifications(true);
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(target, sel!(columnsResized:), Some(NSViewFrameDidChangeNotification), Some(row.as_ref()));
        }
        row
    }

    /// Puts the row's parts over the list's and the note's columns (the split view's second and
    /// third), wherever the dividers are.
    fn lay_out_row(&self) {
        let row = self.ivars().row.borrow();
        let Some(row) = row.as_ref() else { return };
        let columns = self.ivars().split_view.arrangedSubviews();
        for (part, column) in [(&row.over_sidebar, 0), (&row.over_list, 1), (&row.over_note, 2), (&row.over_panel, 3)] {
            let Some(column) = columns.iter().nth(column) else { continue };
            let over = row.view.convertRect_fromView(column.convertRect_toView(column.bounds(), None), None);
            part.setFrame(NSRect::new(NSPoint::new(over.origin.x, 0.0), NSSize::new(over.size.width, row.view.bounds().size.height)));
        }
        // The list's part goes with its column (hidden during a review, ADR 0037): its name and
        // buttons do not fit in no width.
        let list_shown = columns.iter().nth(1).is_some_and(|column| !column.isHidden() && column.frame().size.width >= 1.0);
        row.over_list.setHidden(!list_shown);
    }

    fn make_item(&self, id: &NSString) -> Option<Retained<NSToolbarItem>> {
        let mtm = self.mtm();
        match id.to_string().as_str() {
            TABS => {
                let strip = self.ivars().tabs.borrow().clone()?;
                let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), id);
                item.setLabel(&NSString::from_str(strings::TABS));
                // As wide as the toolbar lets it between the toggles: the toolbar gives an item
                // whose view may be wider what is left.
                strip.setTranslatesAutoresizingMaskIntoConstraints(false);
                let wide = strip.widthAnchor().constraintEqualToConstant(4000.0);
                wide.setPriority(200.0);
                NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
                    strip.heightAnchor().constraintEqualToConstant(crate::tab_strip::HEIGHT),
                    strip.widthAnchor().constraintGreaterThanOrEqualToConstant(120.0),
                    wide,
                ]));
                item.setView(Some(&strip));
                // No frame of the toolbar's around it: the strip draws its own capsule, and the
                // two, one inside the other and not as tall, showed as a double edge.
                item.setBordered(false);
                Some(item)
            }
            TOOLBAR_SEARCH | TOOLBAR_NEW_NOTE => {
                // The same as the buttons of the row below do, over the list and over the note.
                let (symbol, label, action) = if id.to_string() == TOOLBAR_SEARCH { ("magnifyingglass", strings::SEARCH_VAULT, sel!(searchVault:)) } else { ("square.and.pencil", strings::NEW_NOTE, sel!(newNote:)) };
                let item = button(mtm, id, symbol, label);
                // SAFETY: the delegate outlives its toolbar's items and takes the action.
                unsafe {
                    item.setTarget(Some(self));
                    item.setAction(Some(action));
                }
                Some(item)
            }
            TOOLBAR_ZEN => {
                let item = button(mtm, id, zen_symbol(false), strings::ZEN);
                // SAFETY: the delegate outlives its toolbar's items and takes the action.
                unsafe {
                    item.setTarget(Some(self));
                    item.setAction(Some(sel!(toggleZen:)));
                }
                *self.ivars().zen.borrow_mut() = Some(item.clone());
                Some(item)
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

    /// Lets the name be edited, all of it selected.
    pub fn begin_editing(&self) {
        self.setEditable(true);
        self.setSelectable(true);
        if let Some(window) = self.window() {
            window.makeFirstResponder(Some(self));
        }
    }
}

/// A date formatter writing Ukrainian ("12 листопада 2026 р. о 14:03", "Сьогодні о 07:17"):
/// the region settings' language may be another (`en_US@rg=uazzzz` wrote "12 November 2026 at
/// 14:03" under a Ukrainian name).
fn ukrainian_dates() -> Retained<NSDateFormatter> {
    let format = NSDateFormatter::new();
    format.setLocale(Some(&NSLocale::initWithLocaleIdentifier(NSLocale::alloc(), &NSString::from_str("uk_UA"))));
    format.setDateStyle(NSDateFormatterStyle::LongStyle);
    format.setTimeStyle(NSDateFormatterStyle::ShortStyle);
    format.setDoesRelativeDateFormatting(true);
    format
}

/// The name field is a label again once editing ends.
fn end_editing(field: &NSControl) {
    if let Some(field) = field.downcast_ref::<NSTextField>() {
        field.setEditable(false);
        field.setSelectable(false);
    }
}

fn zen_symbol(on: bool) -> &'static str {
    if on { "arrow.down.right.and.arrow.up.left" } else { "arrow.up.left.and.arrow.down.right" }
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
    // SAFETY: as above.
    let space = unsafe { objc2_app_kit::NSToolbarSpaceItemIdentifier };
    NSArray::from_retained_slice(&[
        system[0].copy(),
        // Between the sidebar's toggle and the panel's: the tabs, a space at each side (next to
        // one another the toolbar puts items on one piece of glass, and the panel's button
        // stood inside the tabs' frame). What belongs to the list and the note is in the row
        // below (`second_row`).
        space.copy(),
        custom(TABS),
        // After the tabs, on one piece of glass: the search and a new note.
        space.copy(),
        custom(TOOLBAR_SEARCH),
        custom(TOOLBAR_NEW_NOTE),
        custom(TOOLBAR_ZEN),
        space.copy(),
        custom(PANEL),
    ])
}

/// `search_field`: the list's search field, which the row under the toolbar shows in the place
/// of the list's name.
pub fn install(window: &objc2_app_kit::NSWindow, delegate: &ToolbarDelegate, search_field: &NSSearchField, tabs: &NSView) {
    *delegate.ivars().tabs.borrow_mut() = Some(objc2::Message::retain(tabs));
    let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(window.mtm()), ns_string!("KnotesMainToolbar"));
    toolbar.setDelegate(Some(ProtocolObject::from_ref(delegate)));
    toolbar.setDisplayMode(objc2_app_kit::NSToolbarDisplayMode::IconOnly);
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(objc2_app_kit::NSWindowToolbarStyle::Unified);
    // The list's name is in the row below, not beside the toolbar's buttons.
    window.setTitleVisibility(objc2_app_kit::NSWindowTitleVisibility::Hidden);
    let accessory = objc2_app_kit::NSTitlebarAccessoryViewController::new(window.mtm());
    accessory.setView(&delegate.second_row(search_field));
    accessory.setLayoutAttribute(NSLayoutAttribute::Bottom);
    window.addTitlebarAccessoryViewController(&accessory);
    delegate.lay_out_row();
}

/// The control `id` of the row under `window`'s toolbar (probe mode).
pub fn row_control(window: &objc2_app_kit::NSWindow, id: &str) -> Option<Retained<NSView>> {
    fn find(view: &NSView, id: &str) -> Option<Retained<NSView>> {
        view.subviews().iter().find_map(|v| if v.identifier().is_some_and(|i| i.to_string() == id) { Some(v) } else { find(&v, id) })
    }
    window.titlebarAccessoryViewControllers().iter().find_map(|accessory| find(&accessory.view(), id))
}

/// The row under the toolbar.
struct Row {
    view: Retained<NSView>,
    over_sidebar: Retained<NSView>,
    /// Folders, tags, cards.
    modes: [Retained<NSButton>; 3],
    over_panel: Retained<NSView>,
    /// The AI, the outline, the links.
    panels: [Retained<NSButton>; 5],
    over_list: Retained<NSView>,
    over_note: Retained<NSView>,
    list_title: Retained<NSTextField>,
    list_count: Retained<NSTextField>,
    /// Empties the recently deleted.
    empty: Retained<NSButton>,
    back: Retained<NSButton>,
    forward: Retained<NSButton>,
    /// Where the note's name stands: after the arrows, or in the middle (zen mode).
    title_left: Retained<NSLayoutConstraint>,
    title_middle: Retained<NSLayoutConstraint>,
}

/// The button `index` on and in the accent colour, the others off and grey.
fn choose(buttons: &[Retained<NSButton>], index: isize) {
    for (i, button) in buttons.iter().enumerate() {
        let chosen = i as isize == index;
        button.setState(if chosen { NSControlStateValueOn } else { NSControlStateValueOff });
        let tint = if chosen { NSColor::controlAccentColor() } else { NSColor::secondaryLabelColor() };
        button.setContentTintColor(Some(&tint));
    }
}

/// The row's buttons for what the sidebar lists: tabs, folders, tags, the cards' decks.
pub const SIDEBAR_MODE_IDS: [&str; 3] = ["knotes.sidebar-folders", "knotes.sidebar-tags", "knotes.sidebar-cards"];

/// The row's buttons for what the panel shows: the AI, the outline, the links.
pub const PANEL_MODE_IDS: [&str; 5] = ["knotes.panel-stars", "knotes.panel-outline", "knotes.panel-links", "knotes.panel-calendar", "knotes.panel-ai"];

/// The row's height.
const ROW_HEIGHT: f64 = 28.0;

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
        SidebarMode::Cards => 2,
    }
}

fn panel_index(mode: PanelMode) -> isize {
    match mode {
        PanelMode::Stars => 0,
        PanelMode::Outline => 1,
        PanelMode::Links => 2,
        PanelMode::Calendar => 3,
        PanelMode::Ai => 4,
    }
}

