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
    static ACTED: Cell<bool> = const { Cell::new(false) };
}

/// True the first time only: requested actions run on the first snapshot, not on the ones
/// that follow the app's own saves.
pub fn act_once() -> bool {
    !ACTED.with(|a| a.replace(true))
}

/// Types `KNOTES_PROBE_TYPE` the way the keyboard inserts text: at the end of the note, or
/// over `KNOTES_PROBE_RANGE=location,length` (UTF-16; empty text there deletes). In one
/// insertion, as a paste or an input method does, or with `KNOTES_PROBE_TYPE_CHARS=1` one
/// character after another, as keys do.
pub fn type_requested(view: &objc2_app_kit::NSTextView) {
    let Ok(text) = std::env::var("KNOTES_PROBE_TYPE") else { return };
    let range = std::env::var("KNOTES_PROBE_RANGE").ok().and_then(|r| {
        let (location, length) = r.split_once(',')?;
        Some(objc2_foundation::NSRange::new(location.parse().ok()?, length.parse().ok()?))
    });
    let range = range.unwrap_or_else(|| objc2_foundation::NSRange::new(view.string().length(), 0));
    view.setSelectedRange(range);
    let not_found = objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0);
    let pieces: Vec<String> =
        if std::env::var("KNOTES_PROBE_TYPE_CHARS").is_ok() { text.chars().map(String::from).collect() } else { vec![text] };
    for piece in pieces {
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&piece), not_found) };
    }
}

/// `KNOTES_PROBE_PASTE=file.png`: that image's data pasted into the note (with
/// `KNOTES_PROBE_PASTE_TEXT=text` beside it); `KNOTES_PROBE_DROP=a.png,b.txt@index`: files
/// dropped at character `index`. On a pasteboard of its own, never the user's.
pub fn paste_requested(view: &crate::note_view::NoteTextView) {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardWriting};
    let pasteboard = NSPasteboard::pasteboardWithUniqueName();
    pasteboard.clearContents();
    let at = if let Ok(file) = std::env::var("KNOTES_PROBE_PASTE") {
        let bytes = std::fs::read(&file).expect("the probe's image");
        // SAFETY: AppKit's pasteboard type constants live for the whole process.
        unsafe { pasteboard.setData_forType(Some(&objc2_foundation::NSData::with_bytes(&bytes)), NSPasteboardTypePNG) };
        if let Ok(text) = std::env::var("KNOTES_PROBE_PASTE_TEXT") {
            // SAFETY: as above.
            pasteboard.setString_forType(&NSString::from_str(&text), unsafe { NSPasteboardTypeString });
        }
        None
    } else if let Ok(drop) = std::env::var("KNOTES_PROBE_DROP") {
        let (files, at) = drop.rsplit_once('@').expect("files@index");
        let urls: Vec<_> = files
            .split(',')
            .map(|f| ProtocolObject::<dyn NSPasteboardWriting>::from_retained(objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(f))))
            .collect();
        pasteboard.writeObjects(&objc2_foundation::NSArray::from_retained_slice(&urls));
        at.parse().ok()
    } else {
        return;
    };
    println!("pasted-images: {}", if view.offer_paste(&pasteboard, at) { "yes" } else { "no" });
}

/// `KNOTES_PROBE_EDGE=x,y;x,y`: whether the pointer at each point (text view coordinates)
/// would resize an image (`resize`) or not (`none`).
pub fn image_edge_requested(view: &crate::note_view::NoteTextView) {
    let Ok(points) = std::env::var("KNOTES_PROBE_EDGE") else { return };
    let answers: Vec<&str> = points
        .split(';')
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            let point = objc2_foundation::NSPoint::new(x.parse().ok()?, y.parse().ok()?);
            Some(if view.image_edge_at(point).is_some() { "resize" } else { "none" })
        })
        .collect();
    println!("image-edge: {}", answers.join(" "));
}

/// `KNOTES_PROBE_RESIZE=line:width`: the image on the line starting at character `line`
/// dragged to `width` points and let go.
pub fn resize_requested() -> Option<(usize, f64)> {
    let request = std::env::var("KNOTES_PROBE_RESIZE").ok()?;
    let (line, width) = request.split_once(':')?;
    Some((line.parse().ok()?, width.parse().ok()?))
}

/// `KNOTES_PROBE_NOTE_SCROLL=end`: scrolls the note to its end, as the scroll wheel does.
pub fn note_scroll_requested(view: &objc2_app_kit::NSTextView) {
    if std::env::var("KNOTES_PROBE_NOTE_SCROLL").as_deref() != Ok("end") {
        return;
    }
    let end = view.string().length();
    view.scrollRangeToVisible(objc2_foundation::NSRange::new(end, 0));
}

/// `KNOTES_PROBE_KEYS=down,return,…`: key commands as the keyboard sends them to the text
/// view (`doCommandBySelector:`, which asks the delegate first).
pub fn keys_requested(view: &objc2_app_kit::NSTextView) {
    use objc2::sel;
    let Ok(keys) = std::env::var("KNOTES_PROBE_KEYS") else { return };
    for key in keys.split(',') {
        let command = match key {
            "down" => sel!(moveDown:),
            "up" => sel!(moveUp:),
            "return" => sel!(insertNewline:),
            "tab" => sel!(insertTab:),
            "escape" => sel!(cancelOperation:),
            other => panic!("unknown probe key {other}"),
        };
        // SAFETY: a key-binding selector the text view implements.
        unsafe { objc2_app_kit::NSStandardKeyBindingResponding::doCommandBySelector(view, command) };
    }
}

/// `KNOTES_PROBE_CLICK=index` or `index:cmd`: a mouse click on that character, delivered to
/// the text view as AppKit would (a mouse-up is queued for its tracking loop).
pub fn click_requested(view: &crate::note_view::NoteTextView) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSResponder};
    let Ok(spec) = std::env::var("KNOTES_PROBE_CLICK") else { return };
    let (index, cmd) = spec.split_once(':').map_or((spec.as_str(), false), |(i, m)| (i, m == "cmd"));
    let (Ok(index), Some(window)) = (index.parse::<usize>(), view.window()) else { return };
    // Right after opening, TextKit 2 has not laid out the lines yet: their rects would all be
    // the first line's.
    if let Some(layout) = view.textLayoutManager() {
        layout.textViewportLayoutController().layoutViewport();
    }
    // SAFETY: the range lies inside the text; a null actual range is allowed.
    let rect = unsafe {
        objc2_app_kit::NSTextInputClient::firstRectForCharacterRange_actualRange(&**view, objc2_foundation::NSRange::new(index, 1), std::ptr::null_mut())
    };
    let center = objc2_foundation::NSPoint::new(rect.origin.x + rect.size.width / 2.0, rect.origin.y + rect.size.height / 2.0);
    let at = window.convertPointFromScreen(center);
    let flags = if cmd { NSEventModifierFlags::Command } else { NSEventModifierFlags::empty() };
    let event = |kind| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind, at, flags, 0.0, window.windowNumber(), None, 0, 1, 1.0,
        )
    };
    let (Some(down), Some(up)) = (event(NSEventType::LeftMouseDown), event(NSEventType::LeftMouseUp)) else { return };
    NSApplication::sharedApplication(view.mtm()).postEvent_atStart(&up, false);
    NSResponder::mouseDown(view, &down);
}

/// `KNOTES_PROBE_WRITE=text`: another program rewrites the open note's file.
pub fn write_requested(window: &VaultWindow) {
    let (Ok(text), Some(path), Some(vault)) = (std::env::var("KNOTES_PROBE_WRITE"), window.open_path(), window.vault()) else { return };
    std::fs::write(vault.join(path), text).expect("outside write");
}

/// `KNOTES_PROBE_REWRITE_IMAGE=pic.png=/path/new.png`: another program writes a new picture
/// over an image in the vault.
pub fn rewrite_image_requested(window: &VaultWindow) {
    let (Ok(request), Some(vault)) = (std::env::var("KNOTES_PROBE_REWRITE_IMAGE"), window.vault()) else { return };
    let Some((path, from)) = request.split_once('=') else { return };
    std::fs::copy(from, vault.join(path)).expect("outside write");
}

/// `KNOTES_PROBE_ACTION=sidebar=tags` or `sidebar=folders`: a click on that segment of the
/// toolbar switch, sent to its target as AppKit does.
pub fn sidebar_mode_requested(window: &NSWindow) {
    let index = match std::env::var("KNOTES_PROBE_ACTION").as_deref() {
        Ok("sidebar=folders") => 0,
        Ok("sidebar=tags") => 1,
        _ => return,
    };
    let Some(group) = sidebar_switch(window) else { return };
    group.setSelectedIndex(index);
    if let Some(action) = group.action() {
        let target = group.target();
        // SAFETY: the group's own action and target, with the group as sender.
        unsafe { NSApplication::sharedApplication(window.mtm()).sendAction_to_from(action, target.as_deref(), Some(&group)) };
    }
}

fn sidebar_switch(window: &NSWindow) -> Option<objc2::rc::Retained<NSToolbarItemGroup>> {
    let id = NSString::from_str(crate::toolbar::SIDEBAR_MODE);
    window.toolbar()?.items().iter().find(|i| i.itemIdentifier().isEqualToString(&id))?.downcast::<NSToolbarItemGroup>().ok()
}

/// `KNOTES_PROBE_ACTION=toggle-sidebar`: the toolbar's sidebar button action, sent to the
/// split view controller (in probe mode the window is not key, so the responder chain would
/// not reach it).
pub fn toggle_sidebar_requested(window: &NSWindow) {
    let action = std::env::var("KNOTES_PROBE_ACTION");
    let twice = action.as_deref() == Ok("toggle-sidebar-twice");
    if action.as_deref() != Ok("toggle-sidebar") && !twice {
        return;
    }
    let Some(controller) = window.contentViewController() else { return };
    let toggle = |controller: &objc2_app_kit::NSViewController| {
        // SAFETY: NSSplitViewController implements toggleSidebar:.
        unsafe { NSApplication::sharedApplication(controller.mtm()).sendAction_to_from(objc2::sel!(toggleSidebar:), Some(controller), None) };
    };
    toggle(&controller);
    if twice {
        // `toggle-sidebar-twice`: back again once it has gone.
        let target = dispatch2::MainThreadBound::new(controller, window.mtm());
        let when = dispatch2::DispatchTime::try_from(std::time::Duration::from_millis(700)).expect("short delay");
        let _ = dispatch2::DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            toggle(target.get(mtm));
        });
    }
}

/// `KNOTES_PROBE_SEARCH=text`: typed into the search field over the list, then sent as the
/// field sends it.
pub fn search_requested(window: &NSWindow) {
    let Ok(query) = std::env::var("KNOTES_PROBE_SEARCH") else { return };
    let id = NSString::from_str(crate::toolbar::SEARCH);
    let Some(item) = window.toolbar().and_then(|t| t.items().iter().find(|i| i.itemIdentifier().isEqualToString(&id))) else { return };
    // A click on the magnifying glass, then typing into the field it shows.
    if let Some(action) = item.action() {
        let target = item.target();
        // SAFETY: the item's own action and target.
        unsafe { NSApplication::sharedApplication(window.mtm()).sendAction_to_from(action, target.as_deref(), Some(&item)) };
    }
    let Some(field) = search_field(window) else { return };
    if std::env::var_os("KNOTES_PROBE_SEARCH_TYPE").is_some() {
        // Typed key by key into the field editor, as the keyboard does.
        window.makeFirstResponder(Some(&field));
        if let Some(editor) = field.currentEditor().and_then(|e| e.downcast::<objc2_app_kit::NSTextView>().ok()) {
            for c in query.chars() {
                let not_found = objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0);
                // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
                unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(&*editor, &NSString::from_str(&c.to_string()), not_found) };
            }
        }
        println!("search: {query}");
        return;
    }
    field.setStringValue(&NSString::from_str(&query));
    // KNOTES_PROBE_SEARCH_MODE=content: then a click on that segment of the switch.
    let switch_to = std::env::var("KNOTES_PROBE_SEARCH_MODE").ok();
    if let Some(action) = field.action() {
        let target = field.target();
        // SAFETY: the field's own action and target, with the field as sender.
        unsafe { NSApplication::sharedApplication(window.mtm()).sendAction_to_from(action, target.as_deref(), Some(&field)) };
    }
    if let Some(mode) = switch_to
        && let Some(switch) = search_switch(window)
    {
        switch.setSelectedSegment(if mode == "content" { 1 } else { 0 });
        if let Some(action) = switch.action() {
            let target = switch.target();
            // SAFETY: the switch's own action and target, with the switch as sender.
            unsafe { NSApplication::sharedApplication(window.mtm()).sendAction_to_from(action, target.as_deref(), Some(&switch)) };
        }
    }
    println!("search: {query}");
}

/// The files/content switch of the search row.
fn search_switch(window: &NSWindow) -> Option<objc2::rc::Retained<objc2_app_kit::NSSegmentedControl>> {
    fn find(view: &objc2_app_kit::NSView) -> Option<objc2::rc::Retained<objc2_app_kit::NSSegmentedControl>> {
        if let Some(control) = view.downcast_ref::<objc2_app_kit::NSSegmentedControl>() {
            return Some(objc2::Message::retain(control));
        }
        view.subviews().iter().find_map(|sub| find(&sub))
    }
    let content = window.contentView()?;
    find(&content)
}

/// Mode shown by the search switch, the text in the search field, and whether the end of the
/// text (where typing goes on) is inside the field's visible text area.
fn report_search(window: &NSWindow) {
    if let (Some(switch), Some(field)) = (search_switch(window), search_field(window)) {
        println!("search-mode: {}", if switch.selectedSegment() == 1 { "content" } else { "files" });
        println!("search-field: {}", field.stringValue());
        // The field editor sits in a clip view as wide as the field's text area; the end of the
        // text is in view when the clip shows the editor's right edge.
        if let Some(editor) = field.currentEditor().and_then(|e| e.downcast::<objc2_app_kit::NSTextView>().ok())
            && let Some(clip) = unsafe { editor.superview() }
        {
            let (shown, frame) = (clip.bounds(), editor.frame());
            let end_visible = shown.origin.x + shown.size.width >= frame.origin.x + frame.size.width - 2.0;
            println!("search-caret-visible: {end_visible}");
        }
    }
}

/// The search field of the list column, if the window has one.
fn search_field(window: &NSWindow) -> Option<objc2::rc::Retained<objc2_app_kit::NSSearchField>> {
    fn find(view: &objc2_app_kit::NSView) -> Option<objc2::rc::Retained<objc2_app_kit::NSSearchField>> {
        if let Some(field) = view.downcast_ref::<objc2_app_kit::NSSearchField>() {
            return Some(objc2::Message::retain(field));
        }
        view.subviews().iter().find_map(|sub| find(&sub))
    }
    let content = window.contentView()?;
    find(&content)
}

/// `KNOTES_PROBE_ACTION=new`: File → New Note through the responder chain, like the menu.
/// True when it was requested.
pub fn new_note_requested() -> bool {
    if std::env::var("KNOTES_PROBE_ACTION").as_deref() != Ok("new") {
        return false;
    }
    let mtm = MainThreadMarker::new().expect("main thread");
    // SAFETY: a nil target sends the action up the responder chain.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(objc2::sel!(newNote:), None, None) };
    true
}

/// `KNOTES_PROBE_ACTION=rename=New name`: the open note renamed to that name.
pub fn rename_requested() -> Option<String> {
    std::env::var("KNOTES_PROBE_ACTION").ok()?.strip_prefix("rename=").map(str::to_owned)
}

/// `KNOTES_PROBE_ACTION=list-rename=New name`: Rename from the note list's context menu on the
/// selected note, the name typed in its row and the editing ended. `list-move=folder`: Move
/// to… from that menu (the folder picker answers with the folder).
pub fn list_menu_requested(list: &crate::note_list_view::NoteListSource, table: &objc2_app_kit::NSTableView) {
    let Ok(action) = std::env::var("KNOTES_PROBE_ACTION") else { return };
    let (index, name) = match (action.strip_prefix("list-rename="), action.starts_with("list-move=")) {
        (Some(name), _) => (0, Some(name.to_owned())),
        (None, true) => (1, None),
        _ => return,
    };
    let Some(menu) = table.menu() else { return };
    let items = menu.itemArray();
    println!("list-menu-items: {}", items.iter().map(|i| i.title().to_string()).collect::<Vec<_>>().join(" | "));
    let Some(item) = items.iter().nth(index) else { return };
    let mtm = MainThreadMarker::new().expect("main thread");
    // SAFETY: the menu item's own target and action, sent as a click would.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(item.action().expect("menu action"), item.target().as_deref(), Some(&item)) };
    if let (Some(name), Some(field)) = (name, list.renaming_field()) {
        field.setStringValue(&NSString::from_str(&name));
        if let Some(window) = table.window() {
            window.makeFirstResponder(Some(table));
        }
    }
}

/// `KNOTES_PROBE_ACTION=drag-to=folder`: the selected note dragged onto that sidebar folder:
/// its row writes the pasteboard and the folder takes the drop, as in a drag (the gesture
/// itself cannot be made here). Reports which rows take a note.
pub fn drag_requested(list: &crate::note_list_view::NoteListSource, table: &objc2_app_kit::NSTableView, sidebar: &crate::sidebar_view::SidebarSource) {
    use knotes_app::SidebarItem;
    let Some(folder) = std::env::var("KNOTES_PROBE_ACTION").ok().and_then(|a| a.strip_prefix("drag-to=").map(str::to_owned)) else { return };
    let targets = [(folder.clone(), SidebarItem::Folder(folder.clone())), ("All notes".to_owned(), SidebarItem::AllNotes)];
    let shown: Vec<String> = targets
        .iter()
        .map(|(name, item)| format!("{name}={}", if crate::sidebar_view::drop_folder(item).is_some() { "move" } else { "none" }))
        .collect();
    println!("drop-targets: {}", shown.join(" "));
    let Some(writer) = list.note_writer(table.selectedRow()) else { return };
    let pasteboard = objc2_app_kit::NSPasteboard::pasteboardWithUniqueName();
    pasteboard.clearContents();
    pasteboard.writeObjects(&objc2_foundation::NSArray::from_retained_slice(&[writer]));
    sidebar.accept_note_drop(&pasteboard, &SidebarItem::Folder(folder));
}

/// `KNOTES_PROBE_ACTION=folder-rename=Folder=New name`: Rename from the sidebar's context menu
/// on that folder's row, the name typed in it and the editing ended.
pub fn folder_rename_requested(sidebar: &crate::sidebar_view::SidebarSource, outline: &objc2_app_kit::NSOutlineView) {
    let Some(spec) = std::env::var("KNOTES_PROBE_ACTION").ok().and_then(|a| a.strip_prefix("folder-rename=").map(str::to_owned)) else { return };
    let Some((folder, name)) = spec.split_once('=') else { return };
    let object = sidebar.object_for(&knotes_app::SidebarItem::Folder(folder.to_owned()));
    // SAFETY: an item object of this outline view.
    let row = unsafe { outline.rowForItem(Some(&object)) };
    if row < 0 {
        return;
    }
    outline.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row as usize), false);
    let Some(menu) = outline.menu() else { return };
    let items = menu.itemArray();
    println!("sidebar-menu-items: {}", items.iter().map(|i| i.title().to_string()).collect::<Vec<_>>().join(" | "));
    let Some(item) = items.iter().next() else { return };
    let mtm = MainThreadMarker::new().expect("main thread");
    // SAFETY: the menu item's own target and action, sent as a click would.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(item.action().expect("menu action"), item.target().as_deref(), Some(&item)) };
    let Some(field) = sidebar.renaming_field() else { return };
    field.setStringValue(&NSString::from_str(name));
    if let Some(window) = outline.window() {
        window.makeFirstResponder(Some(outline));
    }
}

/// `KNOTES_PROBE_SHORTCUT=f` (or `shift+f`): Cmd with that key pressed while the note has the
/// focus, through the window and the menu bar as AppKit sends a key equivalent.
pub fn shortcut_requested(window: &NSWindow, focus: &objc2_app_kit::NSView) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    let Ok(spec) = std::env::var("KNOTES_PROBE_SHORTCUT") else { return };
    let (shift, key) = spec.strip_prefix("shift+").map_or((false, spec.as_str()), |k| (true, k));
    window.makeFirstResponder(Some(focus));
    let flags = if shift { NSEventModifierFlags::Command | NSEventModifierFlags::Shift } else { NSEventModifierFlags::Command };
    let chars = NSString::from_str(key);
    let Some(event) = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown, objc2_foundation::NSPoint::new(0.0, 0.0), flags, 0.0, window.windowNumber(), None, &chars, &chars, false, 3,
    ) else { return };
    let handled = window.performKeyEquivalent(&event)
        || NSApplication::sharedApplication(window.mtm()).mainMenu().is_some_and(|menu| menu.performKeyEquivalent(&event));
    println!("shortcut-handled: {handled}");
}

/// `KNOTES_PROBE_MENU=Item title`: that menu bar item chosen, as a click on it would.
pub fn menu_requested() {
    let Ok(title) = std::env::var("KNOTES_PROBE_MENU") else { return };
    let mtm = MainThreadMarker::new().expect("main thread");
    let Some(bar) = NSApplication::sharedApplication(mtm).mainMenu() else { return };
    for menu in bar.itemArray().iter().filter_map(|i| i.submenu()) {
        if let Some(index) = menu.itemArray().iter().position(|i| i.title().to_string() == title) {
            menu.performActionForItemAtIndex(index as isize);
            return;
        }
    }
}

/// `KNOTES_PROBE_ACTION=save`: File → Save, sent through the responder chain like the menu.
pub fn save_requested() {
    if std::env::var("KNOTES_PROBE_ACTION").as_deref() != Ok("save") {
        return;
    }
    let mtm = MainThreadMarker::new().expect("main thread");
    // SAFETY: a nil target sends the action up the responder chain.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(objc2::sel!(saveDocument:), None, None) };
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
        "subfolders=on" => Some(ListOption::IncludeSubfolders(true)),
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
    let delay = std::env::var("KNOTES_PROBE_DELAY_MS").ok().and_then(|d| d.parse().ok()).unwrap_or(500);
    let when = DispatchTime::try_from(Duration::from_millis(delay)).expect("short delay");
    let _ = DispatchQueue::main().after(when, move || {
        let mtm = MainThreadMarker::new().expect("main queue");
        WINDOW.with(|w| {
            if let Some(window) = w.borrow().as_ref() {
                report(&window.window);
                let placeholder = window.chat.question.placeholderString().map(|p| p.to_string()).unwrap_or_default();
                println!("chat: {} | {placeholder}", window.chat.title.stringValue());
                if std::env::var("KNOTES_PROBE_ACTION").as_deref() == Ok("scroll-bench") {
                    scroll_bench(&window.table);
                }
                if std::env::var("KNOTES_PROBE_ACTION").as_deref() == Ok("editor-bench") {
                    editor_bench(window);
                }
                if with_sidebar {
                    report_sidebar(&window.outline);
                    report_sidebar_mode(&window.window);
                    report_list_separator(&window.window);
                    report_search(&window.window);
                    report_list(&window.table);
                    println!("list-menu: {}", window.list_menu.describe());
                    report_note(window);
                }
                if let Some(path) = std::env::var_os("KNOTES_PROBE_PNG") {
                    render_png(&window.window, &path.to_string_lossy());
                }
                if let Some(path) = std::env::var_os("KNOTES_PROBE_PANEL_PNG")
                    && let Some(panel) = window.window.childWindows().and_then(|c| c.firstObject())
                {
                    render_png(&panel, &path.to_string_lossy());
                }
            }
        });
        if std::env::var("KNOTES_PROBE_ACTION").as_deref() == Ok("close") {
            // The app must quit by itself once its last window closes.
            println!("closing window");
            WINDOW.with(|w| {
                if let Some(window) = w.borrow_mut().take() {
                    window.window.performClose(None);
                }
            });
        } else {
            NSApplication::sharedApplication(mtm).terminate(None);
        }
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
    if let Some(field) = note_title_field(window) {
        println!("note-title: {}", field.stringValue());
    }
}

/// The open note's name field in the toolbar.
fn note_title_field(window: &NSWindow) -> Option<objc2::rc::Retained<objc2_app_kit::NSTextField>> {
    let id = NSString::from_str(crate::toolbar::NOTE_TITLE);
    let item = window.toolbar()?.items().iter().find(|i| i.itemIdentifier().isEqualToString(&id))?;
    item.view()?.downcast::<objc2_app_kit::NSTextField>().ok()
}

/// `KNOTES_PROBE_ACTION=toolbar-rename=New name`: the name typed over the open note's in the
/// toolbar and Return pressed.
pub fn toolbar_rename_requested(window: &NSWindow) {
    let Some(name) = std::env::var("KNOTES_PROBE_ACTION").ok().and_then(|a| a.strip_prefix("toolbar-rename=").map(str::to_owned)) else { return };
    let Some(field) = note_title_field(window) else { return };
    field.setStringValue(&NSString::from_str(&name));
    let mtm = MainThreadMarker::new().expect("main thread");
    // SAFETY: the field's own target and action, sent as Return does.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(field.action().expect("field action"), field.target().as_deref(), Some(&field)) };
}

/// Physical footprint of this process (what Activity Monitor shows as Memory), in MB.
fn footprint_mb() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        uuid: [u8; 16],
        user_time: u64,
        system_time: u64,
        pkg_idle_wkups: u64,
        interrupt_wkups: u64,
        pageins: u64,
        wired_size: u64,
        resident_size: u64,
        phys_footprint: u64,
        proc_start_abstime: u64,
        proc_exit_abstime: u64,
    }
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV0) -> i32;
    }
    let mut info = RusageInfoV0::default();
    // SAFETY: flavor 0 (RUSAGE_INFO_V0) fills exactly this struct for our own pid.
    let ok = unsafe { proc_pid_rusage(std::process::id() as i32, 0, &mut info) } == 0;
    if ok { info.phys_footprint as f64 / (1 << 20) as f64 } else { 0.0 }
}

/// Scroll pockets of the list and note columns (inside our scroll views) and how many of
/// them still draw a separator line.
fn report_pockets(window: &NSWindow) {
    fn walk(view: &objc2_app_kit::NSView, found: &mut (usize, usize)) {
        if crate::quiet_scroll_view::is_pocket(view)
            && unsafe { view.superview() }.is_some_and(|s| s.downcast_ref::<crate::quiet_scroll_view::QuietScrollView>().is_some())
        {
            found.0 += 1;
            // SAFETY: `is_pocket` checked the class, which answers allowsSeparator.
            let on: bool = unsafe { objc2::msg_send![view, allowsSeparator] };
            found.1 += usize::from(on);
        }
        for sub in view.subviews().iter() {
            walk(&sub, found);
        }
    }
    let mut found = (0, 0);
    if let Some(content) = window.contentView() {
        walk(&content, &mut found);
    }
    println!("column-pockets: {}, separators on: {}", found.0, found.1);
}

fn report(window: &NSWindow) {
    report_toolbar(window);
    report_pockets(window);
    println!("memory: footprint {:.1} MB", footprint_mb());
    let (menus, file) = crate::main_menu::describe(&NSApplication::sharedApplication(window.mtm()));
    println!("menu: {menus}");
    println!("file-menu: {file}");
    println!("edit-menu: {}", crate::main_menu::describe_edit(&NSApplication::sharedApplication(window.mtm())));
    println!("title: {}", window.title());
    println!("subtitle: {}", window.subtitle());
    let frame = window.frame();
    println!("size: {:.0}x{:.0}", frame.size.width, frame.size.height);
    println!("titlebar: {}", if window.titlebarAppearsTransparent() { "transparent" } else { "opaque" });
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
/// x of the toolbar separator line over the list/note boundary, and of that boundary.
fn report_list_separator(window: &NSWindow) {
    fn separators(view: &objc2_app_kit::NSView, out: &mut Vec<f64>) {
        if view.class().name().to_str().unwrap_or("").contains("SeparatorToolbarItemView") {
            let f = view.convertRect_toView(view.bounds(), None);
            out.push(f.origin.x + f.size.width / 2.0);
        }
        for sub in view.subviews().iter() {
            separators(&sub, out);
        }
    }
    let Some(split) = window.contentViewController().and_then(|c| c.downcast::<NSSplitViewController>().ok()) else { return };
    let item = split.splitViewItems().objectAtIndex(1);
    let list = item.viewController(window.mtm()).view();
    let boundary = list.convertRect_toView(list.bounds(), None);
    let boundary = boundary.origin.x + boundary.size.width;
    let mut lines = Vec::new();
    if let Some(frame) = window.contentView().and_then(|c| unsafe { c.superview() }) {
        separators(&frame, &mut lines);
    }
    let nearest = lines.iter().copied().min_by(|a, b| (a - boundary).abs().total_cmp(&(b - boundary).abs())).unwrap_or(f64::NAN);
    println!("list-separator: {nearest:.1} boundary {boundary:.1}");
}

fn report_sidebar_mode(window: &NSWindow) {
    let mode = match sidebar_switch(window).map(|g| (g.isHidden(), g.selectedIndex())) {
        Some((true, _)) => "hidden",
        Some((false, 0)) => "folders",
        Some((false, 1)) => "tags",
        _ => "none",
    };
    let collapsed = window
        .contentViewController()
        .and_then(|c| c.downcast::<NSSplitViewController>().ok())
        .and_then(|split| split.splitViewItems().firstObject())
        .is_some_and(|item| item.isCollapsed());
    println!("sidebar-mode: {mode}{}", if collapsed { " (sidebar collapsed)" } else { "" });
}

fn report_sidebar(outline: &NSOutlineView) {
    let rows: Vec<String> = (0..outline.numberOfRows())
        .filter_map(|row| {
            let view = outline.viewAtColumn_row_makeIfNecessary(0, row, true)?;
            // The gap row is a plain view.
            let Ok(cell) = view.downcast::<NSTableCellView>() else { return Some("·".to_owned()) };
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
    println!("textkit: {}", if window.note_text.textLayoutManager().is_some() { 2 } else { 1 });
    println!("editor: {}", crate::live_preview::LivePreview::describe(&window.note_text));
    println!("editor-struck: {}", crate::live_preview::LivePreview::describe_struck(&window.note_text));
    println!("editor-shaded: {}", crate::live_preview::LivePreview::describe_shaded(&window.note_text));
    println!("formula-ink: {}", window.formula_ink());
    for line in window.describe_drawn() {
        println!("{line}");
    }
    if let Some(scroll) = window.note_text.enclosingScrollView() {
        let shown = scroll.contentView().bounds();
        println!("note-visible: {:.1} {:.1}", shown.origin.y, shown.origin.y + shown.size.height);
    }
    let view = &window.note_text;
    let container = unsafe { view.textContainer() }.map_or(0.0, |c| c.size().width);
    println!("text-layout: column={:.1} width={:.1} left={:.1}", view.frame().size.width, container, view.textContainerInset().width);
    let (path, _, _) = window.describe_note();
    if let Some(path) = path {
        let text: String = window.note_text.string().to_string().replace('\n', "⏎").chars().take(60).collect();
        println!("note: {path} | {text}");
    }
    println!("document: {}", if window.is_dirty() { "dirty" } else { "clean" });
    println!("completion: {}", window.describe_completion());
    let responder = window.window.firstResponder();
    let is = |view: *const u8| responder.as_ref().is_some_and(|r| std::ptr::eq(&**r as *const _ as *const u8, view));
    // The search field edits in the window's field editor, whose delegate is the field.
    let editing = |field: *const u8| {
        responder
            .as_ref()
            .and_then(|r| r.downcast_ref::<objc2_app_kit::NSTextView>().and_then(|t| t.delegate()))
            .is_some_and(|d| std::ptr::eq(&*d as *const _ as *const u8, field))
    };
    let focus = if is(&*window.note_text as *const _ as *const u8) {
        "editor"
    } else if editing(&*window.search_bar.field as *const _ as *const u8) {
        "search"
    } else if editing(&*window.find_bar.field as *const _ as *const u8) {
        "find"
    } else {
        "other"
    };
    println!("focus: {focus}");
    {
        println!("find-bar: {}", if window.find_bar.is_shown() { "shown" } else { "hidden" });
        println!("find: {}", window.find_bar.count_text());
        // The menu validates its items against the focused note, as before a key press.
        window.window.makeFirstResponder(Some(&window.note_text));
        let mtm = window.window.mtm();
        if let Some(edit) = NSApplication::sharedApplication(mtm).mainMenu().and_then(|bar| bar.itemArray().iter().nth(2)).and_then(|i| i.submenu()) {
            edit.update();
            let finds: Vec<String> = edit
                .itemArray()
                .iter()
                .filter(|i| i.action() == Some(objc2::sel!(performTextFinderAction:)))
                .map(|i| format!("{}={}", i.title(), if i.isEnabled() { "on" } else { "off" }))
                .collect();
            println!("find-menu: {}", finds.join(" "));
        }
    }
    println!("search-row: {}", if window.search_bar.is_shown() { "shown" } else { "hidden" });
    println!("cursor: {}", window.note_text.selectedRange().location);
    if let Some(disk) = window.disk_text() {
        println!("disk: {disk:?}");
    }
    let history = NSString::from_str(crate::toolbar::HISTORY);
    if let Some(group) = window.window.toolbar().and_then(|t| {
        t.items().iter().find(|i| i.itemIdentifier().isEqualToString(&history)).and_then(|i| i.downcast::<NSToolbarItemGroup>().ok())
    }) {
        let state: Vec<&str> = group.subitems().iter().map(|i| if i.isEnabled() { "on" } else { "off" }).collect();
        println!("nav: back={} forward={}", state.first().unwrap_or(&"?"), state.get(1).unwrap_or(&"?"));
    }
    // What a look at the window decodes: the images drawn in view.
    window.window.displayIfNeeded();
    let (decoded, kept) = window.decoded_images();
    println!("images-decoded: {decoded}");
    println!("images-kept: {kept}");
    let row = window.table.selectedRow();
    if row >= 0
        && let Some(cell) = window.table.viewAtColumn_row_makeIfNecessary(0, row, true).and_then(|v| v.downcast::<NSTableCellView>().ok())
        && let Some(title) = field(&cell, TITLE_ID)
    {
        println!("selected-row: {}", title.stringValue());
    }
}

/// Scrolls the note list in 40-point steps (a fast flick at 120 Hz) and times the main-thread
/// work of each step: scrolling, laying out and filling newly visible rows, drawing.
fn scroll_bench(table: &NSTableView) {
    let Some(scroll) = table.enclosingScrollView() else { return };
    let clip = scroll.contentView();
    let height = table.frame().size.height - clip.bounds().size.height;
    let steps = ((height / 40.0) as usize).clamp(1, 3000);
    let mut times = Vec::with_capacity(steps);
    for i in 0..steps {
        let start = std::time::Instant::now();
        clip.scrollToPoint(objc2_foundation::NSPoint::new(0.0, (i as f64 * 40.0).min(height)));
        scroll.reflectScrolledClipView(&clip);
        table.layoutSubtreeIfNeeded();
        scroll.displayIfNeeded();
        times.push(start.elapsed());
    }
    times.sort_unstable();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
    println!(
        "scroll-bench: steps={steps} p50={:.2}ms p95={:.2}ms max={:.2}ms (120 Hz frame: 8.33 ms)",
        ms(times[steps / 2]),
        ms(times[steps * 95 / 100]),
        ms(times[steps - 1])
    );
}

fn quantiles(mut times: Vec<std::time::Duration>) -> String {
    times.sort_unstable();
    let n = times.len().max(1);
    let ms = |d: Option<&std::time::Duration>| d.map_or(0.0, |d| d.as_secs_f64() * 1e3);
    format!("p50={:.2}ms p95={:.2}ms max={:.2}ms n={}", ms(times.get(n / 2)), ms(times.get(n * 95 / 100)), ms(times.last()), times.len())
}

/// Editor costs on the open note: open+style, cursor moves, scrolling, full restyle.
fn editor_bench(window: &VaultWindow) {
    use objc2_foundation::{NSPoint, NSRange};
    let view: &objc2_app_kit::NSTextView = &window.note_text;
    let source = view.string().to_string();
    let t = std::time::Instant::now();
    window.reopen_current();
    let open = t.elapsed();
    let ns = view.string();
    // Cursor moves: one paragraph down at a time, scrolling to keep it visible.
    let (mut at, mut moves) = (0usize, Vec::new());
    while at < ns.length() && moves.len() < 400 {
        let next = ns.paragraphRangeForRange(NSRange::new(at, 0));
        at = next.location + next.length;
        let t = std::time::Instant::now();
        view.setSelectedRange(NSRange::new(at.min(ns.length()), 0));
        view.scrollRangeToVisible(NSRange::new(at.min(ns.length()), 0));
        view.layoutSubtreeIfNeeded();
        if let Some(scroll) = view.enclosingScrollView() {
            scroll.displayIfNeeded();
        }
        moves.push(t.elapsed());
    }
    // Scrolling the whole note in 40-point steps.
    let mut scrolls = Vec::new();
    if let Some(scroll) = view.enclosingScrollView() {
        let clip = scroll.contentView();
        let height = view.frame().size.height - clip.bounds().size.height;
        let mut y = 0.0;
        while y < height && scrolls.len() < 3000 {
            let t = std::time::Instant::now();
            clip.scrollToPoint(NSPoint::new(0.0, y));
            scroll.reflectScrolledClipView(&clip);
            view.layoutSubtreeIfNeeded();
            scroll.displayIfNeeded();
            scrolls.push(t.elapsed());
            y += 40.0;
        }
    }
    // Images and formulas drawn all the way down stay decoded.
    println!("editor-bench: footprint after scrolling {:.1} MB", footprint_mb());
    // Worst case of an edit: highlight and restyle the whole note.
    let mut restyles = Vec::new();
    for _ in 0..10 {
        let t = std::time::Instant::now();
        window.live_preview_restyle(&source);
        restyles.push(t.elapsed());
    }
    // Typing in the middle of the note, a key at a time, then deleting it all again (so the
    // note matches the file and quitting writes nothing).
    let middle = ns.paragraphRangeForRange(NSRange::new(ns.length() / 2, 0));
    let start = middle.location + middle.length.saturating_sub(1);
    view.setSelectedRange(NSRange::new(start, 0));
    view.scrollRangeToVisible(NSRange::new(start, 0));
    let mut keys = Vec::new();
    let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
    for c in "typing **bold** and `code` in the middle ".chars().cycle().take(200) {
        let t = std::time::Instant::now();
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&c.to_string()), not_found) };
        view.layoutSubtreeIfNeeded();
        if let Some(scroll) = view.enclosingScrollView() {
            scroll.displayIfNeeded();
        }
        keys.push(t.elapsed());
    }
    view.setSelectedRange(NSRange::new(start, 200));
    // SAFETY: as above.
    unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(""), not_found) };
    println!("editor-bench: {} bytes; open+style {:.2}ms", source.len(), open.as_secs_f64() * 1e3);
    println!("editor-bench: keystroke {}", quantiles(keys));
    println!("editor-bench: cursor move {}", quantiles(moves));
    println!("editor-bench: scroll {}", quantiles(scrolls));
    println!("editor-bench: full restyle {}", quantiles(restyles));
}
