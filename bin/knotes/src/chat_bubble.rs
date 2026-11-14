//! The user's questions in the chat's conversation, set apart from the answers: a question's
//! paragraph gets a TextKit 2 layout fragment that draws a tinted rounded card behind it, across
//! the column (ADR 0027). The text stays one text view: selection and copying go across.

use objc2::rc::Retained;
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_app_kit::{NSBezierPath, NSColor, NSGraphicsContext, NSTextElement, NSTextElementProvider, NSTextLayoutFragment, NSTextLocation, NSTextParagraph, NSTextRange, NSTextView};
use objc2_core_graphics::CGContext;
use objc2_foundation::{NSAttributedString, NSComparisonResult, NSObject, NSPoint, NSRect, NSSize, NSString};

/// The attribute marking a question's text.
pub const QUESTION: &str = "knotes.chat.question";
/// The card's padding above and below the question's lines (its sides are the paragraph's
/// indents, `CARD_INDENT`).
const CARD_PADDING: f64 = 7.0;
/// The question's text inset from the card's sides.
pub const CARD_INDENT: f64 = 8.0;
const CARD_CORNER: f64 = 10.0;

define_class!(
    #[unsafe(super(NSTextLayoutFragment, NSObject))]
    #[name = "KnotesChatBubble"]
    pub struct ChatBubble;

    impl ChatBubble {
        #[unsafe(method(drawAtPoint:inContext:))]
        fn draw(&self, point: NSPoint, context: &CGContext) {
            let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(context, true);
            let previous = NSGraphicsContext::currentContext();
            NSGraphicsContext::setCurrentContext(Some(&graphics));
            let card = self.card();
            let card = NSRect::new(NSPoint::new(point.x + card.origin.x, point.y + card.origin.y), card.size);
            NSColor::controlAccentColor().colorWithAlphaComponent(0.12).setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, CARD_CORNER, CARD_CORNER).fill();
            NSGraphicsContext::setCurrentContext(previous.as_deref());
            // SAFETY: the superclass draws the text, as it would without this override.
            let _: () = unsafe { msg_send![super(self), drawAtPoint: point, inContext: context] };
        }

        #[unsafe(method(renderingSurfaceBounds))]
        fn rendering_surface_bounds(&self) -> NSRect {
            // SAFETY: the superclass's bounds of the text.
            let text: NSRect = unsafe { msg_send![super(self), renderingSurfaceBounds] };
            union(text, self.card())
        }
    }
);

impl ChatBubble {
    pub fn new(element: &NSTextElement, range: Option<&NSTextRange>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: NSTextLayoutFragment's designated initializer.
        unsafe { msg_send![super(this), initWithTextElement: element, range: range] }
    }

    /// The card, in the fragment's coordinates: across the text column, a little taller than
    /// the question's lines.
    fn card(&self) -> NSRect {
        let frame = self.layoutFragmentFrame();
        let column = self.textLayoutManager().and_then(|l| l.textContainer()).map_or(frame.size.width, |c| c.size().width);
        NSRect::new(NSPoint::new(-frame.origin.x, -CARD_PADDING), NSSize::new(column, frame.size.height + 2.0 * CARD_PADDING))
    }
}

/// Whether a paragraph is a question (its text starts with the question attribute).
pub fn is_question(element: &NSTextElement) -> bool {
    let Some(paragraph) = element.downcast_ref::<NSTextParagraph>() else { return false };
    let text: Retained<NSAttributedString> = paragraph.attributedString();
    // SAFETY: an index inside the string.
    text.length() > 0 && unsafe { text.attribute_atIndex_effectiveRange(&NSString::from_str(QUESTION), 0, std::ptr::null_mut()) }.is_some()
}

/// The questions drawn as cards in `view` (probe lines), laid out first.
pub fn cards(view: &NSTextView) -> Vec<String> {
    let Some(layout) = view.textLayoutManager() else { return Vec::new() };
    let Some(content) = layout.textContentManager() else { return Vec::new() };
    let whole = content.documentRange();
    layout.ensureLayoutForRange(&whole);
    let mut cards = Vec::new();
    let mut at = whole.location();
    while let Some(fragment) = layout.textLayoutFragmentForLocation(&at) {
        if fragment.downcast_ref::<ChatBubble>().is_some()
            && let Some(paragraph) = fragment.textElement().and_then(|e| e.downcast::<NSTextParagraph>().ok())
        {
            cards.push(paragraph.attributedString().string().to_string().trim_end().to_string());
        }
        let next = fragment.rangeInElement().endLocation();
        if next.compare(&whole.endLocation()) != NSComparisonResult::Ascending || next.compare(&at) != NSComparisonResult::Descending {
            break;
        }
        at = next;
    }
    cards
}

fn union(a: NSRect, b: NSRect) -> NSRect {
    let x0 = a.origin.x.min(b.origin.x);
    let y0 = a.origin.y.min(b.origin.y);
    let x1 = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
    NSRect::new(NSPoint::new(x0, y0), NSSize::new(x1 - x0, y1 - y0))
}
