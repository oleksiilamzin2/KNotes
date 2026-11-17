//! The space switcher (ADR 0030): a pop-up at the foot of the sidebar offering the whole vault,
//! the spaces with their dots and the spaces' settings. Hidden while the vault has no space,
//! the folder tree then reaching down to the bottom.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSImage, NSLayoutConstraint, NSMenuItem, NSPopUpButton, NSScrollView, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};

use knotes_spaces::Space;

use crate::strings;

type OnSpace = Box<dyn Fn(Option<String>)>;
type OnSettings = Box<dyn Fn()>;

pub struct Ivars {
    popup: RefCell<Option<Retained<NSPopUpButton>>>,
    /// The spaces offered and the window's.
    spaces: RefCell<(Vec<Space>, Option<String>)>,
    on_space: RefCell<Option<OnSpace>>,
    on_settings: RefCell<Option<OnSettings>>,
    /// The folder tree's bottom: on the pop-up while it shows, on the column's foot else.
    above: RefCell<Option<Retained<NSLayoutConstraint>>>,
    alone: RefCell<Option<Retained<NSLayoutConstraint>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSpaceSwitcher"]
    #[ivars = Ivars]
    pub struct SpaceSwitcher;

    unsafe impl NSObjectProtocol for SpaceSwitcher {}

    impl SpaceSwitcher {
        /// An item of the pop-up chosen: a space, the whole vault, or the settings (after which
        /// the window's space shows again).
        #[unsafe(method(chooseSpace:))]
        fn choose(&self, sender: &NSPopUpButton) {
            let Some(item) = sender.selectedItem() else { return };
            let spec = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string());
            match spec.as_deref() {
                Some("settings") => {
                    self.show();
                    if let Some(callback) = self.ivars().on_settings.borrow().as_ref() {
                        callback();
                    }
                }
                Some(id) => {
                    let id = id.strip_prefix("s:").map(str::to_owned);
                    if let Some(callback) = self.ivars().on_space.borrow().as_ref() {
                        callback(id);
                    }
                }
                None => {}
            }
        }
    }
);

impl SpaceSwitcher {
    /// The sidebar's column: `tree` (the folder tree's scroll view) above the switcher.
    pub fn column(mtm: MainThreadMarker, tree: &NSScrollView) -> (Retained<Self>, Retained<NSView>) {
        let this: Retained<Self> = {
            let this = Self::alloc(mtm).set_ivars(Ivars {
                popup: RefCell::new(None),
                spaces: RefCell::new((Vec::new(), None)),
                on_space: RefCell::new(None),
                on_settings: RefCell::new(None),
                above: RefCell::new(None),
                alone: RefCell::new(None),
            });
            unsafe { msg_send![super(this), init] }
        };
        let column = NSView::new(mtm);
        let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), objc2_foundation::NSRect::ZERO, false);
        popup.setToolTip(Some(&NSString::from_str(strings::SPACE)));
        // SAFETY: the switcher outlives the pop-up (the window keeps both) and implements it.
        unsafe {
            popup.setTarget(Some(&this));
            popup.setAction(Some(sel!(chooseSpace:)));
        }
        for view in [tree as &NSView, &popup] {
            view.setTranslatesAutoresizingMaskIntoConstraints(false);
            column.addSubview(view);
        }
        let above = tree.bottomAnchor().constraintEqualToAnchor_constant(&popup.topAnchor(), -6.0);
        let alone = tree.bottomAnchor().constraintEqualToAnchor(&column.bottomAnchor());
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            tree.topAnchor().constraintEqualToAnchor(&column.topAnchor()),
            tree.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            tree.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            popup.leadingAnchor().constraintEqualToAnchor_constant(&column.leadingAnchor(), 10.0),
            popup.trailingAnchor().constraintEqualToAnchor_constant(&column.trailingAnchor(), -10.0),
            popup.bottomAnchor().constraintEqualToAnchor_constant(&column.safeAreaLayoutGuide().bottomAnchor(), -10.0),
            alone.clone(),
        ]));
        *this.ivars().popup.borrow_mut() = Some(popup);
        *this.ivars().above.borrow_mut() = Some(above);
        *this.ivars().alone.borrow_mut() = Some(alone);
        this.show();
        (this, column)
    }

    pub fn on_space(&self, callback: impl Fn(Option<String>) + 'static) {
        *self.ivars().on_space.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_settings(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_settings.borrow_mut() = Some(Box::new(callback));
    }

    /// The spaces to offer and the window's.
    pub fn set_spaces(&self, spaces: &[Space], current: Option<&str>) {
        *self.ivars().spaces.borrow_mut() = (spaces.to_vec(), current.map(str::to_owned));
        self.show();
    }

    /// The pop-up's items (the whole vault, the spaces with their dots, the settings), the
    /// window's space selected; hidden, and the tree to the bottom, while there are no spaces.
    fn show(&self) {
        let Some(popup) = self.ivars().popup.borrow().clone() else { return };
        let mtm = self.mtm();
        let (spaces, current) = self.ivars().spaces.borrow().clone();
        let menu = popup.menu().expect("a pop-up has a menu");
        menu.removeAllItems();
        let entry = |title: &str, spec: &str| {
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), None, &NSString::new()) };
            // SAFETY: a string, which the item keeps as it is.
            unsafe { item.setRepresentedObject(Some(NSString::from_str(spec).as_ref())) };
            item
        };
        // Monochrome (owner's choice): the whole vault a book, a space braces, not the tabs'
        // stacked squares and the spaces' coloured dots.
        let symbol = |name: &str, description: &str| NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), Some(&NSString::from_str(description)));
        let whole = entry(strings::WHOLE_VAULT, "");
        whole.setImage(symbol(crate::path_bar::VAULT_SYMBOL, strings::WHOLE_VAULT).as_deref());
        menu.addItem(&whole);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let mut selected = 0;
        for space in &spaces {
            let item = entry(&space.name, &format!("s:{}", space.id));
            item.setImage(symbol("curlybraces", &space.name).as_deref());
            if current.as_deref() == Some(space.id.as_str()) {
                selected = menu.numberOfItems();
            }
            menu.addItem(&item);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&entry(strings::SPACE_SETTINGS, "settings"));
        popup.selectItemAtIndex(selected);
        let hidden = spaces.is_empty();
        popup.setHidden(hidden);
        if let (Some(above), Some(alone)) = (self.ivars().above.borrow().as_ref(), self.ivars().alone.borrow().as_ref()) {
            // Off first, so the two never hold at once.
            if hidden {
                above.setActive(false);
                alone.setActive(true);
            } else {
                alone.setActive(false);
                above.setActive(true);
            }
        }
    }

    /// The switcher for the probe: "Уся база | ✓ Робота | Дім | — | Налаштувати простори…" (the
    /// selected item ticked), or "(hidden)".
    pub fn probe_menu(&self) -> String {
        let Some(popup) = self.ivars().popup.borrow().clone() else { return String::new() };
        if popup.isHidden() {
            return "(hidden)".into();
        }
        let selected = popup.indexOfSelectedItem();
        let items: Vec<String> = (0..popup.numberOfItems())
            .filter_map(|i| popup.itemAtIndex(i).map(|item| (i, item)))
            .filter(|(i, item)| !(item.isSeparatorItem() && *i == 1))
            .map(|(i, item)| if item.isSeparatorItem() { "—".to_string() } else { format!("{}{}", if i == selected { "✓ " } else { "" }, item.title()) })
            .collect();
        items.join(" | ")
    }

    /// Chooses a space from the pop-up, as a click does (probe mode).
    pub fn probe_choose(&self, id: Option<&str>) {
        let Some(popup) = self.ivars().popup.borrow().clone() else { return };
        let wanted = id.map_or(String::new(), |id| format!("s:{id}"));
        let at = (0..popup.numberOfItems()).find(|&i| popup.itemAtIndex(i).and_then(|item| item.representedObject()).and_then(|o| o.downcast::<NSString>().ok()).is_some_and(|s| s.to_string() == wanted));
        if let Some(at) = at {
            popup.selectItemAtIndex(at);
            let target: &AnyObject = self.as_ref();
            // SAFETY: the pop-up's own action, sent to its target as a choice does.
            let _: () = unsafe { msg_send![target, chooseSpace: &*popup] };
        }
    }
}
