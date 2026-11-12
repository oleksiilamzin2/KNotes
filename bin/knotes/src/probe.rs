//! Probe mode (`KNOTES_PROBE=1`): print the window structure and quit, for tests.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSBitmapImageFileType, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarInspectorTrackingSeparatorItemIdentifier, NSToolbarItemGroup, NSToolbarSidebarTrackingSeparatorItemIdentifier, NSToolbarToggleSidebarItemIdentifier, NSOutlineView, NSSplitViewController, NSSplitViewItemBehavior, NSTableCellView, NSTableView, NSWindow};

use objc2_foundation::{NSDictionary, NSString};

use crate::note_list_view::{SNIPPET_ID, TIME_ID, TITLE_ID, field};
use crate::sidebar_view::count_field;
use crate::vault_window::VaultWindow;

thread_local! {
    /// The window to report on; main thread only.
    static WINDOW: RefCell<Option<Rc<VaultWindow>>> = const { RefCell::new(None) };
    static SCHEDULED: Cell<bool> = const { Cell::new(false) };
}

pub fn enabled() -> bool {
    std::env::var_os("KNOTES_PROBE").is_some()
}

/// Sidebar item to select before reporting (`KNOTES_PROBE_SELECT=f:Folder` or `t:tag`).
pub fn requested_selection() -> Option<knotes_app::SidebarItem> {
    knotes_app::SidebarItem::from_key(&std::env::var("KNOTES_PROBE_SELECT").ok()?)
}

/// List option to choose before reporting (`KNOTES_PROBE_ACTION=sort=title`, `group=off`, …),
/// through the same handler as the menu.
pub fn requested_option() -> Option<knotes_app::ListOption> {
    use knotes_app::{ListOption, SortOrder};
    match std::env::var("KNOTES_PROBE_ACTION").ok()?.as_str() {
        "sort=title" => Some(ListOption::SortBy(SortOrder::Title)),
        "sort=modified" => Some(ListOption::SortBy(SortOrder::Modified)),
        "group=off" => Some(ListOption::GroupByDate(false)),
        "subfolders=off" => Some(ListOption::IncludeSubfolders(false)),
        _ => None,
    }
}

/// Notes to open one after another, as clicks in the list (`KNOTES_PROBE_OPEN=a.md,b.md`).
pub fn requested_notes() -> Vec<String> {
    std::env::var("KNOTES_PROBE_OPEN").map(|v| v.split(',').map(str::to_owned).collect()).unwrap_or_default()
}

/// `KNOTES_PROBE_ACTION=back` or `forward`: the toolbar's history buttons.
pub fn requested_navigation() -> Option<bool> {
    match std::env::var("KNOTES_PROBE_ACTION").ok()?.as_str() {
        "back" => Some(false),
        "forward" => Some(true),
        _ => None,
    }
}

pub fn watch(window: &Rc<VaultWindow>) {
    WINDOW.with(|w| *w.borrow_mut() = Some(Rc::clone(window)));
}

/// Reports half a second later, after AppKit's first layout and display passes (window
/// sizing happens there), then quits. Later calls are ignored.
pub fn schedule(with_sidebar: bool) {
    if SCHEDULED.with(|s| s.replace(true)) {
        return;
    }
    let when = DispatchTime::try_from(Duration::from_millis(500)).expect("short delay");
    let _ = DispatchQueue::main().after(when, move || {
        let mtm = MainThreadMarker::new().expect("main queue");
        WINDOW.with(|w| {
            if let Some(window) = w.borrow().as_ref() {
                report(&window.window);
                let placeholder = window.chat.question.placeholderString().map(|p| p.to_string()).unwrap_or_default();
                println!("chat: {} | {placeholder}", window.chat.title.stringValue());
                if with_sidebar {
                    report_sidebar(&window.outline);
                    report_list(&window.table);
                    println!("list-menu: {}", window.list_menu.describe());
                    report_note(window);
                }
                if let Some(path) = std::env::var_os("KNOTES_PROBE_PNG") {
                    render_png(&window.window, &path.to_string_lossy());
                }
            }
        });
        NSApplication::sharedApplication(mtm).terminate(None);
    });
}

fn report_toolbar(window: &NSWindow) {
    let Some(toolbar) = window.toolbar() else { return };
    // SAFETY: AppKit's identifier constants are immutable strings for the process lifetime.
    let system = unsafe {
        [
            (NSToolbarToggleSidebarItemIdentifier, "toggle-sidebar"),
            (NSToolbarSidebarTrackingSeparatorItemIdentifier, "sidebar-separator"),
            (NSToolbarFlexibleSpaceItemIdentifier, "flexible-space"),
            (NSToolbarInspectorTrackingSeparatorItemIdentifier, "inspector-separator"),
        ]
    };
    let names: Vec<String> = toolbar
        .items()
        .iter()
        .map(|item| {
            let id = item.itemIdentifier();
            if let Some((_, name)) = system.iter().find(|(s, _)| id.isEqualToString(s)) {
                return (*name).to_owned();
            }
            let short = id.to_string().trim_start_matches("knotes.").to_owned();
            match item.downcast_ref::<NSToolbarItemGroup>() {
                Some(group) => format!("{short}({})", group.subitems().count()),
                None => short,
            }
        })
        .collect();
    println!("toolbar: {}", names.join(", "));
}

fn report(window: &NSWindow) {
    report_toolbar(window);
    let (menus, file) = crate::main_menu::describe(&NSApplication::sharedApplication(window.mtm()));
    println!("menu: {menus}");
    println!("file-menu: {file}");
    println!("title: {}", window.title());
    println!("subtitle: {}", window.subtitle());
    let frame = window.frame();
    println!("size: {:.0}x{:.0}", frame.size.width, frame.size.height);
    if let Some(split) = window.contentViewController().and_then(|c| c.downcast::<NSSplitViewController>().ok()) {
        let roles: Vec<&str> = split
            .splitViewItems()
            .iter()
            .map(|item| match item.behavior() {
                NSSplitViewItemBehavior::Sidebar => "sidebar",
                NSSplitViewItemBehavior::ContentList => "content-list",
                NSSplitViewItemBehavior::Inspector => "inspector",
                _ => "content",
            })
            .collect();
        println!("columns: {}", roles.join(", "));
        let widths: Vec<String> =
            split.splitViewItems().iter().map(|item| format!("{:.0}", item.viewController(window.mtm()).view().frame().size.width)).collect();
        println!("widths: {}", widths.join(" "));
    }
}

/// Visible sidebar rows as `name (count)`, read back from the outline view's cells.
fn report_sidebar(outline: &NSOutlineView) {
    let rows: Vec<String> = (0..outline.numberOfRows())
        .filter_map(|row| {
            let cell = outline.viewAtColumn_row_makeIfNecessary(0, row, true)?.downcast::<NSTableCellView>().ok()?;
            let name = unsafe { cell.textField() }?.stringValue().to_string();
            let count = count_field(&cell).map(|c| c.stringValue().to_string()).unwrap_or_default();
            Some(if count.is_empty() { name } else { format!("{name} ({count})") })
        })
        .collect();
    println!("sidebar: {}", rows.join(" | "));
}

/// Draws the window content into a PNG without the window server (no screen capture, no
/// focus stealing). Layout and text are exact; system materials may look flatter.
fn render_png(window: &NSWindow, path: &str) {
    let Some(view) = window.contentView() else { return };
    let bounds = view.bounds();
    let Some(rep) = view.bitmapImageRepForCachingDisplayInRect(bounds) else { return };
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) };
    if let Some(png) = png {
        png.writeToFile_atomically(&NSString::from_str(path), true);
    }
}

/// First rows of the note list: `[header]` or `title ~ time ~ snippet`, read from the cells.
fn report_list(table: &NSTableView) {
    let rows: Vec<String> = (0..table.numberOfRows().min(40))
        .filter_map(|row| {
            let cell = table.viewAtColumn_row_makeIfNecessary(0, row, true)?.downcast::<NSTableCellView>().ok()?;
            let text = |id: &str| field(&cell, id).map(|f| f.stringValue().to_string());
            match text(TITLE_ID) {
                Some(title) => Some(format!("{title} ~ {} ~ {}", text(TIME_ID).unwrap_or_default(), text(SNIPPET_ID).unwrap_or_default())),
                None => Some(format!("[{}]", unsafe { cell.textField() }?.stringValue())),
            }
        })
        .collect();
    println!("list: {}", rows.join(" | "));
}

/// Open note (from the text view itself), history buttons (from the toolbar) and list selection.
fn report_note(window: &VaultWindow) {
    let (path, _, _) = window.describe_note();
    if let Some(path) = path {
        let text: String = window.note_text.string().to_string().replace('\n', "⏎").chars().take(60).collect();
        println!("note: {path} | {text}");
    }
    if let Some(group) = window.window.toolbar().and_then(|t| {
        t.items().iter().find_map(|i| i.downcast::<NSToolbarItemGroup>().ok())
    }) {
        let state: Vec<&str> = group.subitems().iter().map(|i| if i.isEnabled() { "on" } else { "off" }).collect();
        println!("nav: back={} forward={}", state.first().unwrap_or(&"?"), state.get(1).unwrap_or(&"?"));
    }
    let row = window.table.selectedRow();
    if row >= 0
        && let Some(cell) = window.table.viewAtColumn_row_makeIfNecessary(0, row, true).and_then(|v| v.downcast::<NSTableCellView>().ok())
        && let Some(title) = field(&cell, TITLE_ID)
    {
        println!("selected-row: {}", title.stringValue());
    }
}
