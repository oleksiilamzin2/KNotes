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
/// over `KNOTES_PROBE_RANGE=location,length` (UTF-16; empty text there deletes).
pub fn type_requested(view: &objc2_app_kit::NSTextView) {
    let Ok(text) = std::env::var("KNOTES_PROBE_TYPE") else { return };
    let range = std::env::var("KNOTES_PROBE_RANGE").ok().and_then(|r| {
        let (location, length) = r.split_once(',')?;
        Some(objc2_foundation::NSRange::new(location.parse().ok()?, length.parse().ok()?))
    });
    let range = range.unwrap_or_else(|| objc2_foundation::NSRange::new(view.string().length(), 0));
    view.setSelectedRange(range);
    let not_found = objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0);
    // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
    unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&text), not_found) };
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

fn report(window: &NSWindow) {
    report_toolbar(window);
    println!("memory: footprint {:.1} MB", footprint_mb());
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
    println!("textkit: {}", if window.note_text.textLayoutManager().is_some() { 2 } else { 1 });
    println!("editor: {}", crate::live_preview::LivePreview::describe(&window.note_text));
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
    let focus = window.window.firstResponder().is_some_and(|r| std::ptr::eq(&*r as *const _ as *const u8, &*window.note_text as *const _ as *const u8));
    println!("focus: {}", if focus { "editor" } else { "other" });
    println!("cursor: {}", window.note_text.selectedRange().location);
    if let Some(disk) = window.disk_text() {
        println!("disk: {disk:?}");
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
