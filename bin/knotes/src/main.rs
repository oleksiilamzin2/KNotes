//! Knotes: the AppKit app (ADR 0002, 0010).

mod app_delegate;
mod chat_view;
mod collapse_observer;
mod completion_panel;
mod list_menu;
mod live_preview;
mod main_menu;
mod note_list_view;
mod note_view;
mod probe;
mod search_bar;
mod search_worker;
mod sidebar_view;
mod strings;
mod toolbar;
mod vault_window;
mod window;

use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

/// Large blocks bypass malloc's cache of freed blocks (ADR 0014).
#[global_allocator]
static ALLOCATOR: knotes_alloc::DirectLarge = knotes_alloc::DirectLarge;

fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = app_delegate::AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
