//! The outline of the document open, in the right panel (ADR 0023).

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSView;

pub struct OutlinePanel {
    pub view: Retained<NSView>,
}

impl OutlinePanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Self { view: NSView::new(mtm) }
    }
}
