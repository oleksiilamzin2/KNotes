//! Knotes: the AppKit app (ADR 0002, 0010).

mod app_delegate;
mod chat_view;
mod collapse_observer;
mod find_bar;
mod image_loader;
mod completion_panel;
mod preview_fragment;
mod list_menu;
mod live_preview;
mod main_menu;
mod note_list_view;
mod note_view;
mod outline_panel;
mod pdf_viewer;
mod probe;
mod quick_look;
mod quiet_scroll_view;
mod rename_dialog;
mod search_bar;
mod search_worker;
mod sidebar_view;
mod strings;
mod text_paint;
mod toolbar;
mod vault_window;
mod window;

use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

/// Large blocks bypass malloc's cache of freed blocks (ADR 0014).
#[global_allocator]
static ALLOCATOR: knotes_alloc::DirectLarge = knotes_alloc::DirectLarge;

/// Letters the math fonts lack (Cyrillic in `\text{…}`) are drawn in this serif face, as
/// MathJax in Obsidian draws them (ADR 0017); other scripts fall back further.
const FORMULA_TEXT_FONT: &str = "/System/Library/Fonts/Supplemental/Times New Roman.ttf";

fn main() {
    if std::env::var_os("RATEX_UNICODE_FONT").is_none() && std::path::Path::new(FORMULA_TEXT_FONT).exists() {
        // SAFETY: no other thread runs yet; RaTeX reads the variable on the first formula.
        unsafe { std::env::set_var("RATEX_UNICODE_FONT", FORMULA_TEXT_FONT) };
    }
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = app_delegate::AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
