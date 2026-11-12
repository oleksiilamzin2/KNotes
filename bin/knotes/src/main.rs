//! Knotes: the AppKit app (ADR 0002, 0010).

mod app_delegate;
mod chat_view;
mod list_menu;
mod note_list_view;
mod note_view;
mod probe;
mod sidebar_view;
mod strings;
mod toolbar;
mod vault_window;
mod window;

use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = app_delegate::AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
