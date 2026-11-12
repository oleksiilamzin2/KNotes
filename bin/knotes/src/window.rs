//! The vault window: four columns in one split view controller with native roles
//! (sidebar with Liquid Glass, note list, note, chat inspector).

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSSplitViewController, NSViewController, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSUserDefaults, ns_string};

use crate::probe;
use crate::vault_window::split_controller;

/// Initial column widths (sidebar, list, chat) when the user has not resized them yet.
const INITIAL_WIDTHS: [f64; 3] = [260.0, 340.0, 320.0];
const SPLIT_AUTOSAVE: &str = "KnotesMainSplit";

pub fn build(
    mtm: MainThreadMarker,
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
) -> (Retained<NSWindow>, Retained<NSSplitViewController>) {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1400.0, 860.0));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), frame, style, NSBackingStoreType::Buffered, false)
    };
    // Rust owns the window; AppKit must not release it again on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(ns_string!("Knotes"));

    let split = split_controller(sidebar, list, note, chat, mtm);
    window.setContentViewController(Some(&split));
    // Setting the controller sizes the window to its (still empty) views; restore the size.
    window.setContentSize(frame.size);
    window.center();
    let split_view = split.splitView();
    // Probe runs come from tests: they must not read or write the user's saved widths
    // (user defaults ignore HOME).
    let saved = !probe::enabled()
        && NSUserDefaults::standardUserDefaults()
            .objectForKey(&NSString::from_str(&format!("NSSplitView Subview Frames {SPLIT_AUTOSAVE}")))
            .is_some();
    if !probe::enabled() {
        split_view.setAutosaveName(Some(&NSString::from_str(SPLIT_AUTOSAVE)));
    }
    if !saved {
        split.view().layoutSubtreeIfNeeded();
        let [sidebar_w, list_w, chat_w] = INITIAL_WIDTHS;
        split_view.setPosition_ofDividerAtIndex(sidebar_w, 0);
        split_view.setPosition_ofDividerAtIndex(sidebar_w + list_w, 1);
        split_view.setPosition_ofDividerAtIndex(frame.size.width - chat_w, 2);
    }
    (window, split)
}
