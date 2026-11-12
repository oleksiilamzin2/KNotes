//! The note column's text view (live preview editing, ADR 0005, 0011).

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSFont, NSScrollView, NSTextView};

pub fn note_view(mtm: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSTextView::scrollableTextView(mtm);
    let text = scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).expect("scrollable text view");
    text.setEditable(true);
    text.setSelectable(true);
    text.setRichText(false);
    text.setAllowsUndo(true);
    // Markdown is plain text: no curly quotes, dashes or replacements behind the user's back.
    text.setAutomaticQuoteSubstitutionEnabled(false);
    text.setAutomaticDashSubstitutionEnabled(false);
    text.setAutomaticTextReplacementEnabled(false);
    text.setContinuousSpellCheckingEnabled(true);
    text.setFont(Some(&NSFont::systemFontOfSize(15.0)));
    (scroll, text)
}
