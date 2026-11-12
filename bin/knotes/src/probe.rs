//! Probe mode (`KNOTES_PROBE=1`): print the window structure and quit, for tests.

use objc2_app_kit::{NSSplitViewController, NSSplitViewItemBehavior, NSWindow};

pub fn enabled() -> bool {
    std::env::var_os("KNOTES_PROBE").is_some()
}

pub fn report(window: &NSWindow) {
    println!("title: {}", window.title());
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
    }
}
