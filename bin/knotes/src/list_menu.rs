//! The "…" menu of the note list (as in Apple Notes): sort order, date groups, subfolders.

use std::cell::RefCell;

use knotes_app::{ListOption, ListSettings, SortOrder};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use crate::strings;

const SORT_MODIFIED: isize = 1;
const SORT_TITLE: isize = 2;
const GROUP_BY_DATE: isize = 3;
const INCLUDE_SUBFOLDERS: isize = 4;

type OnChoose = Box<dyn Fn(ListOption)>;

pub struct Ivars {
    on_choose: RefCell<Option<OnChoose>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesListMenuTarget"]
    #[ivars = Ivars]
    pub struct ListMenuTarget;

    unsafe impl NSObjectProtocol for ListMenuTarget {}

    impl ListMenuTarget {
        #[unsafe(method(chooseOption:))]
        fn choose_option(&self, sender: &NSMenuItem) {
            let on = sender.state() == NSControlStateValueOn;
            let option = match sender.tag() {
                SORT_MODIFIED => ListOption::SortBy(SortOrder::Modified),
                SORT_TITLE => ListOption::SortBy(SortOrder::Title),
                GROUP_BY_DATE => ListOption::GroupByDate(!on),
                INCLUDE_SUBFOLDERS => ListOption::IncludeSubfolders(!on),
                _ => return,
            };
            if let Some(callback) = self.ivars().on_choose.borrow().as_ref() {
                callback(option);
            }
        }
    }
);

/// The menu and the target its items call.
pub struct ListMenu {
    pub menu: Retained<NSMenu>,
    target: Retained<ListMenuTarget>,
    sort_modified: Retained<NSMenuItem>,
    sort_title: Retained<NSMenuItem>,
    group: Retained<NSMenuItem>,
    subfolders: Retained<NSMenuItem>,
}

impl ListMenu {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let target: Retained<ListMenuTarget> = {
            let this = ListMenuTarget::alloc(mtm).set_ivars(Ivars { on_choose: RefCell::new(None) });
            unsafe { msg_send![super(this), init] }
        };
        let item = |title: &str, tag: isize| {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(chooseOption:)), &NSString::new())
            };
            let receiver: &AnyObject = target.as_ref();
            unsafe { item.setTarget(Some(receiver)) };
            item.setTag(tag);
            item
        };
        let (sort_modified, sort_title) = (item(strings::SORT_MODIFIED, SORT_MODIFIED), item(strings::SORT_TITLE, SORT_TITLE));
        let sort_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(strings::SORT_BY));
        sort_menu.addItem(&sort_modified);
        sort_menu.addItem(&sort_title);
        let sort = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(strings::SORT_BY), None, &NSString::new()) };
        sort.setSubmenu(Some(&sort_menu));
        let (group, subfolders) = (item(strings::GROUP_BY_DATE, GROUP_BY_DATE), item(strings::INCLUDE_SUBFOLDERS, INCLUDE_SUBFOLDERS));
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(strings::LIST_OPTIONS));
        menu.setAutoenablesItems(false);
        for i in [&sort, &group, &subfolders] {
            menu.addItem(i);
        }
        Self { menu, target, sort_modified, sort_title, group, subfolders }
    }

    pub fn on_choose(&self, callback: impl Fn(ListOption) + 'static) {
        *self.target.ivars().on_choose.borrow_mut() = Some(Box::new(callback));
    }

    /// Check marks for the shown list; "subfolders" only applies to folders.
    pub fn update(&self, settings: &ListSettings, folder: bool) {
        let state = |on: bool| if on { NSControlStateValueOn } else { NSControlStateValueOff };
        self.sort_modified.setState(state(settings.sort == SortOrder::Modified));
        self.sort_title.setState(state(settings.sort == SortOrder::Title));
        self.group.setState(state(settings.group_by_date));
        self.group.setEnabled(settings.sort == SortOrder::Modified);
        self.subfolders.setState(state(settings.include_subfolders));
        self.subfolders.setEnabled(folder);
    }

    /// `Title > [✓ Sub, Sub] | ✓ Item`, for probe mode.
    pub fn describe(&self) -> String {
        fn label(item: &NSMenuItem) -> String {
            let mark = if item.state() == NSControlStateValueOn { "✓ " } else { "" };
            format!("{mark}{}", item.title())
        }
        self.menu
            .itemArray()
            .iter()
            .map(|item| match item.submenu() {
                Some(sub) => format!("{} > [{}]", item.title(), sub.itemArray().iter().map(|i| label(&i)).collect::<Vec<_>>().join(", ")),
                None => label(&item),
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
