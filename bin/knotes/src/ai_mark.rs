//! The mark of what is kept from the AI (ADR 0036): a small grey brain, struck through, after a
//! note's title in the list and after a folder's or a tab's name in the sidebar, before the dot
//! of a space.

use std::cell::OnceCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCompositingOperation, NSGraphicsContext, NSImage, NSImageSymbolConfiguration, NSImageView, NSLayoutConstraintOrientation, NSLineCapStyle,
    NSStackView, NSUserInterfaceItemIdentification, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

use crate::strings;

/// The marks after a row's name, side by side: a star, the AI's, then a space's dot.
const MARKS_ID: &str = "marks";
/// The AI's mark among them.
const AI_ID: &str = "ai-mark";
/// The star of a starred folder or document among them.
const STAR_ID: &str = "star-mark";
/// The mark's size, in points: the brain's own proportions, about as tall as a title's capitals.
const WIDTH: f64 = 15.0;
const HEIGHT: f64 = 13.0;

thread_local! {
    static IMAGE: OnceCell<Option<Retained<NSImage>>> = const { OnceCell::new() };
}

/// The struck-through brain: a template, which its view tints grey (light or dark).
fn image() -> Option<Retained<NSImage>> {
    IMAGE.with(|image| image.get_or_init(draw).clone())
}

fn draw() -> Option<Retained<NSImage>> {
    // There is no `brain.slash` symbol: the slash is drawn over the brain, a gap cut around it
    // as the symbols' own slashes have.
    let brain = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("brain"), None)?;
    let brain = brain.imageWithSymbolConfiguration(&NSImageSymbolConfiguration::configurationWithPointSize_weight(10.0, 0.23))?;
    let handler = RcBlock::new(move |rect: NSRect| -> Bool {
        let size = brain.size();
        let scale = (rect.size.width / size.width).min(rect.size.height / size.height).min(1.0);
        let (width, height) = (size.width * scale, size.height * scale);
        let at = NSPoint::new(rect.origin.x + (rect.size.width - width) / 2.0, rect.origin.y + (rect.size.height - height) / 2.0);
        brain.drawInRect(NSRect::new(at, NSSize::new(width, height)));
        let slash = NSBezierPath::bezierPath();
        slash.moveToPoint(NSPoint::new(rect.origin.x + 1.5, rect.origin.y + rect.size.height - 1.5));
        slash.lineToPoint(NSPoint::new(rect.origin.x + rect.size.width - 1.5, rect.origin.y + 1.5));
        slash.setLineCapStyle(NSLineCapStyle::Round);
        if let Some(context) = NSGraphicsContext::currentContext() {
            context.setCompositingOperation(NSCompositingOperation::Clear);
            slash.setLineWidth(2.8);
            slash.stroke();
            context.setCompositingOperation(NSCompositingOperation::SourceOver);
        }
        NSColor::blackColor().setStroke();
        slash.setLineWidth(1.1);
        slash.stroke();
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(WIDTH, HEIGHT), false, &handler);
    image.setTemplate(true);
    image.setAccessibilityDescription(Some(&NSString::from_str(strings::KEPT_FROM_AI)));
    Some(image)
}

/// A row's marks after its name: a star, the AI's and `dot`, a hidden one taking no room.
pub fn marks(mtm: MainThreadMarker, dot: &NSImageView) -> Retained<NSStackView> {
    let mark = NSImageView::new(mtm);
    mark.setIdentifier(Some(&NSString::from_str(AI_ID)));
    mark.setImage(image().as_deref());
    mark.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    mark.setToolTip(Some(&NSString::from_str(strings::KEPT_FROM_AI)));
    mark.setHidden(true);
    let star = NSImageView::new(mtm);
    star.setIdentifier(Some(&NSString::from_str(STAR_ID)));
    let filled = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("star.fill"), Some(&NSString::from_str(strings::STARRED)));
    star.setImage(filled.and_then(|image| image.imageWithSymbolConfiguration(&NSImageSymbolConfiguration::configurationWithPointSize_weight(9.0, 0.0))).as_deref());
    star.setContentTintColor(Some(&NSColor::systemYellowColor()));
    star.setToolTip(Some(&NSString::from_str(strings::STARRED)));
    star.setHidden(true);
    let views: [&NSView; 3] = [&star, &mark, dot];
    let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
    stack.setIdentifier(Some(&NSString::from_str(MARKS_ID)));
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
    stack.setSpacing(4.0);
    stack.setDetachesHiddenViews(true);
    stack.setTranslatesAutoresizingMaskIntoConstraints(false);
    stack.setContentCompressionResistancePriority_forOrientation(900.0, NSLayoutConstraintOrientation::Horizontal);
    for (view, width, height) in [(&*star, 12.0, 12.0), (&*mark, WIDTH, HEIGHT), (dot, 9.0, 9.0)] {
        view.widthAnchor().constraintEqualToConstant(width).setActive(true);
        view.heightAnchor().constraintEqualToConstant(height).setActive(true);
    }
    stack
}

fn named(view: &NSView, id: &str) -> bool {
    view.identifier().is_some_and(|i| i.to_string() == id)
}

/// The mark `id` among `cell`'s marks.
fn mark(cell: &NSView, id: &str) -> Option<Retained<NSImageView>> {
    let marks = cell.subviews().iter().find(|v| named(v, MARKS_ID))?.downcast::<NSStackView>().ok()?;
    // A hidden mark is out of the stack's subviews, still among its arranged views.
    marks.arrangedSubviews().iter().find(|v| named(v, id)).and_then(|v| v.downcast::<NSImageView>().ok())
}

/// `cell`'s space dot, built with `dot_id`.
pub fn dot(cell: &NSView, dot_id: &str) -> Option<Retained<NSImageView>> {
    mark(cell, dot_id)
}

/// Shows `cell`'s AI mark or hides it.
pub fn show(cell: &NSView, kept: bool) {
    if let Some(mark) = mark(cell, AI_ID) {
        mark.setHidden(!kept);
    }
}

/// Shows `cell`'s star or hides it.
pub fn show_star(cell: &NSView, starred: bool) {
    if let Some(mark) = mark(cell, STAR_ID) {
        mark.setHidden(!starred);
    }
}

/// Whether `cell` shows its star (probe mode).
pub fn star_shown(cell: &NSView) -> bool {
    mark(cell, STAR_ID).is_some_and(|m| !m.isHidden())
}

/// Whether `cell` shows its AI mark (probe mode).
pub fn shown(cell: &NSView) -> bool {
    mark(cell, AI_ID).is_some_and(|m| !m.isHidden())
}
