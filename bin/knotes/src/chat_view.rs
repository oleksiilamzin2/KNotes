//! The chat column as in Knotes v1: a title and the question field. The chat itself
//! (Sign in with ChatGPT, ADR 0004) is a later stage.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSFont, NSTextField, NSView};
use objc2_foundation::NSString;

use crate::strings;

pub struct ChatView {
    pub view: Retained<NSView>,
    pub title: Retained<NSTextField>,
    pub question: Retained<NSTextField>,
}

pub fn chat_view(mtm: MainThreadMarker) -> ChatView {
    let view = NSView::new(mtm);
    let title = NSTextField::labelWithString(&NSString::from_str(strings::CHAT), mtm);
    title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize() + 2.0)));
    let question = NSTextField::new(mtm);
    question.setPlaceholderString(Some(&NSString::from_str(strings::ASK_NOTES)));
    for v in [&*title as &NSView, &question] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(v);
    }
    let guide = view.safeAreaLayoutGuide();
    for c in [
        title.topAnchor().constraintEqualToAnchor_constant(&guide.topAnchor(), 16.0),
        title.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 16.0),
        question.topAnchor().constraintEqualToAnchor_constant(&title.bottomAnchor(), 10.0),
        question.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 14.0),
        question.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -14.0),
    ] {
        c.setActive(true);
    }
    ChatView { view, title, question }
}
