//! Window toolbar as in Knotes v1: sidebar toggle, list title (window title and subtitle),
//! new note and back/forward over the note, chat toggle over the inspector. Separators track
//! the split view columns, so each group sits above its column; macOS draws the glass.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSMenu, NSMenuToolbarItem, NSSplitView, NSToolbar, NSToolbarDelegate, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarInspectorTrackingSeparatorItemIdentifier,
    NSToolbarItem, NSToolbarItemGroup, NSToolbarItemGroupSelectionMode, NSToolbarSidebarTrackingSeparatorItemIdentifier,
    NSToolbarToggleSidebarItemIdentifier, NSTrackingSeparatorToolbarItem,
};
use objc2_foundation::{NSArray, NSCopying, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::strings;

pub const LIST_SEPARATOR: &str = "knotes.list-separator";
pub const LIST_OPTIONS: &str = "knotes.list-options";
pub const NEW_NOTE: &str = "knotes.new-note";
pub const HISTORY: &str = "knotes.history";
pub const CHAT: &str = "knotes.chat";

type OnNavigate = Box<dyn Fn(bool)>;
type OnNewNote = Box<dyn Fn()>;

pub struct Ivars {
    split_view: Retained<NSSplitView>,
    list_menu: Retained<NSMenu>,
    /// Called with `true` for forward, `false` for back.
    on_navigate: RefCell<Option<OnNavigate>>,
    on_new_note: RefCell<Option<OnNewNote>>,
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
            history: RefCell::new(None),
            history_state: Cell::new((false, false)),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub fn on_new_note(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_new_note.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_navigate(&self, callback: impl Fn(bool) + 'static) {
        *self.ivars().on_navigate.borrow_mut() = Some(Box::new(callback));
    }

    /// Enables the back and forward segments.
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
            LIST_SEPARATOR => Some(Retained::into_super(NSTrackingSeparatorToolbarItem::trackingSeparatorToolbarItemWithIdentifier_splitView_dividerIndex(
                id,
                &self.ivars().split_view,
                1,
            ))),
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
            CHAT => {
                let item = button(mtm, id, "brain", strings::CHAT);
                // Up the responder chain to the split view controller.
                unsafe { item.setAction(Some(sel!(toggleInspector:))) };
                Some(item)
            }
            _ => None,
        }
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
        system[1].copy(),
        system[2].copy(),
        custom(LIST_OPTIONS),
        custom(LIST_SEPARATOR),
        custom(NEW_NOTE),
        custom(HISTORY),
        system[2].copy(),
        system[3].copy(),
        custom(CHAT),
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

fn apply_history(group: &NSToolbarItemGroup, can_back: bool, can_forward: bool) {
    group.setAutovalidates(false);
    for (item, on) in group.subitems().iter().zip([can_back, can_forward]) {
        item.setAutovalidates(false);
        item.setEnabled(on);
    }
}
