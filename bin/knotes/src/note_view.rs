//! The note column: read-only text for now; live preview editing is stage 3 (ADR 0005).

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSFont, NSScrollView, NSTextView};

pub fn note_view(mtm: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSTextView::scrollableTextView(mtm);
    let text = scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).expect("scrollable text view");
    text.setEditable(false);
    text.setSelectable(true);
    text.setRichText(false);
    text.setFont(Some(&NSFont::systemFontOfSize(15.0)));
    (scroll, text)
}
