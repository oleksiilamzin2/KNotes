//! The menu bar. Items send their actions up the responder chain (text view, split view
//! controller, window, then the app delegate), so they work in whichever window is key.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSTextFinderAction};
use objc2_foundation::NSString;

use crate::strings as s;

fn tagged(item: Retained<NSMenuItem>, tag: isize) -> Retained<NSMenuItem> {
    item.setTag(tag);
    item
}

/// Command key plus `extra`.
fn cmd(extra: NSEventModifierFlags) -> NSEventModifierFlags {
    NSEventModifierFlags::Command | extra
}

fn item(mtm: MainThreadMarker, title: &str, action: Sel, key: &str, mods: NSEventModifierFlags) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(action), &NSString::from_str(key))
    };
    item.setKeyEquivalentModifierMask(mods);
    item
}

fn submenu(mtm: MainThreadMarker, bar: &NSMenu, title: &str, items: &[Option<Retained<NSMenuItem>>]) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for entry in items {
        match entry {
            Some(i) => menu.addItem(i),
            None => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        }
    }
    let holder = NSMenuItem::new(mtm);
    holder.setSubmenu(Some(&menu));
    bar.addItem(&holder);
    menu
}

pub fn install(app: &NSApplication, mtm: MainThreadMarker) {
    let none = NSEventModifierFlags::empty();
    let bar = NSMenu::new(mtm);
    submenu(mtm, &bar, s::APP, &[
        Some(item(mtm, s::ABOUT, sel!(orderFrontStandardAboutPanel:), "", none)),
        None,
        Some(item(mtm, s::HIDE, sel!(hide:), "h", cmd(none))),
        Some(item(mtm, s::HIDE_OTHERS, sel!(hideOtherApplications:), "h", cmd(NSEventModifierFlags::Option))),
        Some(item(mtm, s::SHOW_ALL, sel!(unhideAllApplications:), "", none)),
        None,
        Some(item(mtm, s::QUIT, sel!(terminate:), "q", cmd(none))),
    ]);
    submenu(mtm, &bar, s::MENU_FILE, &[
        Some(item(mtm, s::NEW_NOTE, sel!(newNote:), "n", cmd(none))),
        Some(item(mtm, s::OPEN_VAULT, sel!(openVault:), "o", cmd(none))),
        Some(item(mtm, s::SAVE, sel!(saveDocument:), "s", cmd(none))),
        Some(item(mtm, s::CLOSE_WINDOW, sel!(performClose:), "w", cmd(none))),
    ]);
    submenu(mtm, &bar, s::MENU_EDIT, &[
        Some(item(mtm, s::UNDO, sel!(undo:), "z", cmd(none))),
        Some(item(mtm, s::REDO, sel!(redo:), "z", cmd(NSEventModifierFlags::Shift))),
        None,
        Some(item(mtm, s::CUT, sel!(cut:), "x", cmd(none))),
        Some(item(mtm, s::COPY, sel!(copy:), "c", cmd(none))),
        Some(item(mtm, s::PASTE, sel!(paste:), "v", cmd(none))),
        Some(item(mtm, s::SELECT_ALL, sel!(selectAll:), "a", cmd(none))),
        None,
        // The find bar over the open note (the app delegate sends these to it from anywhere).
        Some(tagged(item(mtm, s::FIND, sel!(performTextFinderAction:), "f", cmd(none)), NSTextFinderAction::ShowFindInterface.0)),
        Some(tagged(item(mtm, s::FIND_NEXT, sel!(performTextFinderAction:), "g", cmd(none)), NSTextFinderAction::NextMatch.0)),
        Some(tagged(item(mtm, s::FIND_PREVIOUS, sel!(performTextFinderAction:), "g", cmd(NSEventModifierFlags::Shift)), NSTextFinderAction::PreviousMatch.0)),
        Some(item(mtm, s::SEARCH_NOTES, sel!(searchNotes:), "f", cmd(NSEventModifierFlags::Shift))),
    ]);
    submenu(mtm, &bar, s::MENU_VIEW, &[
        Some(item(mtm, s::TOGGLE_SIDEBAR, sel!(toggleSidebar:), "s", cmd(NSEventModifierFlags::Control))),
        Some(item(mtm, s::TOGGLE_PANEL, sel!(toggleInspector:), "i", cmd(NSEventModifierFlags::Option))),
        None,
        Some(item(mtm, s::FULL_SCREEN, sel!(toggleFullScreen:), "f", cmd(NSEventModifierFlags::Control))),
    ]);
    let window_menu = submenu(mtm, &bar, s::MENU_WINDOW, &[
        Some(item(mtm, s::MINIMIZE, sel!(performMiniaturize:), "m", cmd(none))),
        Some(item(mtm, s::ZOOM, sel!(performZoom:), "", none)),
    ]);
    app.setMainMenu(Some(&bar));
    app.setWindowsMenu(Some(&window_menu));
}

/// `Title | Title` of the menu bar and the File items with shortcuts, for probe mode.
pub fn describe(app: &NSApplication) -> (String, String) {
    let Some(bar) = app.mainMenu() else { return Default::default() };
    let titles: Vec<String> = bar.itemArray().iter().filter_map(|i| i.submenu()).map(|m| m.title().to_string()).collect();
    let file = bar.itemArray().iter().nth(1).and_then(|i| i.submenu()).map(|m| shortcuts(&m));
    (titles.join(" | "), file.unwrap_or_default())
}

/// `Title ⇧⌘K | …` of the Edit menu's items, for probe mode.
pub fn describe_edit(app: &NSApplication) -> String {
    app.mainMenu().and_then(|bar| bar.itemArray().iter().nth(2)).and_then(|i| i.submenu()).map(|m| shortcuts(&m)).unwrap_or_default()
}

fn shortcuts(menu: &NSMenu) -> String {
    menu.itemArray()
        .iter()
        .filter(|i| !i.isSeparatorItem())
        .map(|i| {
            let shift = if i.keyEquivalentModifierMask().contains(NSEventModifierFlags::Shift) { "⇧" } else { "" };
            format!("{} {shift}⌘{}", i.title(), i.keyEquivalent().to_string().to_uppercase())
        })
        .collect::<Vec<_>>()
        .join(" | ")
}
