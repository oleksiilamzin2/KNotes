//! Cards behind parts of the chat's conversation (ADR 0027): the user's questions on a tinted
//! rounded card across the column, set apart from the answers; an answer's code blocks on a grey
//! one. A marked paragraph gets a TextKit 2 layout fragment that draws its card; the text stays
//! one text view, selectable and copied across (code keeps its line breaks).

use objc2::rc::Retained;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_app_kit::{NSBezierPath, NSColor, NSGraphicsContext, NSTextContentStorage, NSTextElement, NSTextElementProvider, NSTextLayoutFragment, NSTextLocation, NSTextParagraph, NSTextRange, NSTextView};
use objc2_core_graphics::CGContext;
use objc2_foundation::{NSAttributedString, NSComparisonResult, NSObject, NSPoint, NSRect, NSSize, NSString};

use crate::preview_fragment::rounded;

/// The attribute marking a question's text.
pub const QUESTION: &str = "knotes.chat.question";
/// The attribute marking the text of an answer's code block.
pub const CODE: &str = "knotes.chat.code";
/// The text inset from a card's sides (the paragraphs' indents).
pub const CARD_INDENT: f64 = 8.0;
/// A question card's padding above and below its lines.
const QUESTION_PADDING: f64 = 7.0;
/// A code card's padding above its first line and below its last.
const CODE_PADDING: f64 = 5.0;
const CARD_CORNER: f64 = 10.0;
const CODE_CORNER: f64 = 6.0;

/// What a paragraph's card is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Question,
    /// A code block's line: the block's first (`top`) and last (`bottom`) have rounded corners
    /// and padding; lines between join into one card.
    Code { top: bool, bottom: bool },
}

pub struct Ivars {
    card: Card,
}

define_class!(
    #[unsafe(super(NSTextLayoutFragment, NSObject))]
    #[name = "KnotesChatBubble"]
    #[ivars = Ivars]
    pub struct ChatBubble;

    impl ChatBubble {
        #[unsafe(method(drawAtPoint:inContext:))]
        fn draw(&self, point: NSPoint, context: &CGContext) {
            let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(context, true);
            let previous = NSGraphicsContext::currentContext();
            NSGraphicsContext::setCurrentContext(Some(&graphics));
            let card = self.card();
            let card = NSRect::new(NSPoint::new(point.x + card.origin.x, point.y + card.origin.y), card.size);
            match self.ivars().card {
                Card::Question => {
                    NSColor::controlAccentColor().colorWithAlphaComponent(0.12).setFill();
                    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, CARD_CORNER, CARD_CORNER).fill();
                }
                Card::Code { top, bottom } => {
                    NSColor::quaternarySystemFillColor().setFill();
                    rounded(card, CODE_CORNER, top, bottom).fill();
                }
            }
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
    pub fn new(element: &NSTextElement, range: Option<&NSTextRange>, card: Card) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { card });
        // SAFETY: NSTextLayoutFragment's designated initializer.
        unsafe { msg_send![super(this), initWithTextElement: element, range: range] }
    }

    /// The card, in the fragment's coordinates: across the text column, as tall as the
    /// paragraph's lines and its padding.
    fn card(&self) -> NSRect {
        let frame = self.layoutFragmentFrame();
        let column = self.textLayoutManager().and_then(|l| l.textContainer()).map_or(frame.size.width, |c| c.size().width);
        let (above, below) = match self.ivars().card {
            Card::Question => (QUESTION_PADDING, QUESTION_PADDING),
            Card::Code { top, bottom } => (if top { CODE_PADDING } else { 0.0 }, if bottom { CODE_PADDING } else { 0.0 }),
        };
        NSRect::new(NSPoint::new(-frame.origin.x, -above), NSSize::new(column, frame.size.height + above + below))
    }
}

/// The card a paragraph gets, if any: a question's, or a code line's (whether the lines before
/// and after it are code too decides its corners).
pub fn card_of(element: &NSTextElement) -> Option<Card> {
    let paragraph = element.downcast_ref::<NSTextParagraph>()?;
    let text: Retained<NSAttributedString> = paragraph.attributedString();
    if marked(&text, QUESTION, 0) {
        return Some(Card::Question);
    }
    if !marked(&text, CODE, 0) {
        return None;
    }
    let storage = element.textContentManager().and_then(|m| m.downcast::<NSTextContentStorage>().ok());
    let (Some(storage), Some(range)) = (storage, element.elementRange()) else { return Some(Card::Code { top: true, bottom: true }) };
    let Some(whole) = storage.attributedString() else { return Some(Card::Code { top: true, bottom: true }) };
    let start = storage.offsetFromLocation_toLocation(&storage.documentRange().location(), &range.location());
    let end = storage.offsetFromLocation_toLocation(&storage.documentRange().location(), &range.endLocation());
    let top = start <= 0 || !marked(&whole, CODE, start as usize - 1);
    let bottom = end < 0 || !marked(&whole, CODE, end as usize);
    Some(Card::Code { top, bottom })
}

/// Whether the character at `at` carries attribute `key`.
fn marked(text: &NSAttributedString, key: &str, at: usize) -> bool {
    // SAFETY: an index inside the string (checked first).
    at < text.length() && unsafe { text.attribute_atIndex_effectiveRange(&NSString::from_str(key), at, std::ptr::null_mut()) }.is_some()
}

/// The cards drawn in `view`, laid out first, with their paragraphs' text (probe lines).
pub fn cards(view: &NSTextView) -> Vec<(Card, String)> {
    let Some(layout) = view.textLayoutManager() else { return Vec::new() };
    let Some(content) = layout.textContentManager() else { return Vec::new() };
    let whole = content.documentRange();
    layout.ensureLayoutForRange(&whole);
    let mut cards = Vec::new();
    let mut at = whole.location();
    while let Some(fragment) = layout.textLayoutFragmentForLocation(&at) {
        if let Some(bubble) = fragment.downcast_ref::<ChatBubble>()
            && let Some(paragraph) = fragment.textElement().and_then(|e| e.downcast::<NSTextParagraph>().ok())
        {
            cards.push((bubble.ivars().card, paragraph.attributedString().string().to_string().trim_end().to_string()));
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
