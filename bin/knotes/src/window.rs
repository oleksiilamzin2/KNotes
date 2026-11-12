//! The vault window: four columns in one split view controller with native roles
//! (sidebar with Liquid Glass, note list, note, chat inspector).

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSSplitViewController, NSSplitViewItem, NSView, NSViewController, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, ns_string};

pub fn build(mtm: MainThreadMarker) -> Retained<NSWindow> {
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

    let split = NSSplitViewController::new(mtm);
    let items = [
        NSSplitViewItem::sidebarWithViewController(&placeholder(mtm)),
        NSSplitViewItem::contentListWithViewController(&placeholder(mtm)),
        NSSplitViewItem::splitViewItemWithViewController(&placeholder(mtm)),
        NSSplitViewItem::inspectorWithViewController(&placeholder(mtm)),
    ];
    for item in &items {
        split.addSplitViewItem(item);
    }
    window.setContentViewController(Some(&split));
    // Setting the controller sizes the window to its (still empty) views; restore the size.
    window.setContentSize(frame.size);
    window.center();
    window
}

fn placeholder(mtm: MainThreadMarker) -> Retained<NSViewController> {
    let controller = NSViewController::new(mtm);
    controller.setView(&NSView::new(mtm));
    controller
}
