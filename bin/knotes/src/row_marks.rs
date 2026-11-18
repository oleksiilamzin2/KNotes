//! A document's marks after its name where the app draws the row itself (the tabs along the
//! toolbar, the vault's search): the dot of its space when that is not the window's (ADR 0030),
//! and a struck-through brain when it is kept from the AI (ADR 0036).

use knotes_spaces::SpaceColor;
use objc2_app_kit::{NSBezierPath, NSColor, NSImage, NSImageSymbolConfiguration};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, ns_string};

pub const DOT: f64 = 7.0;
pub const AI_MARK: f64 = 13.0;

/// The room the marks take after a name.
pub fn width(space: bool, kept_from_ai: bool) -> f64 {
    let dot = if space { DOT + 6.0 } else { 0.0 };
    dot + if kept_from_ai { AI_MARK + 5.0 } else { 0.0 }
}

/// Draws the marks from `after` (where the name ends) on, their middles at `middle` (in a
/// flipped view); the AI's in `grey`.
pub fn draw(space: Option<SpaceColor>, kept_from_ai: bool, after: f64, middle: f64, grey: &NSColor) {
    let mut after = after;
    if let Some(colour) = space {
        crate::space_ui::color(colour).setFill();
        NSBezierPath::bezierPathWithOvalInRect(NSRect::new(NSPoint::new(after + 6.0, middle - DOT / 2.0), NSSize::new(DOT, DOT))).fill();
        after += DOT + 6.0;
    }
    if kept_from_ai {
        let (left, top) = (after + 5.0, middle - AI_MARK / 2.0);
        let tint = NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&[grey.colorWithAlphaComponent(grey.alphaComponent())]));
        let brain = NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("brain"), None).and_then(|image| image.imageWithSymbolConfiguration(&tint));
        if let Some(brain) = brain {
            // In the mark's square, its proportions kept.
            let size = brain.size();
            let scale = (AI_MARK / size.width).min(AI_MARK / size.height);
            let (width, height) = (size.width * scale, size.height * scale);
            brain.drawInRect(NSRect::new(NSPoint::new(left + (AI_MARK - width) / 2.0, top + (AI_MARK - height) / 2.0), NSSize::new(width, height)));
        }
        grey.setStroke();
        let slash = NSBezierPath::bezierPath();
        slash.moveToPoint(NSPoint::new(left + 1.0, top + 1.0));
        slash.lineToPoint(NSPoint::new(left + AI_MARK - 1.0, top + AI_MARK - 1.0));
        slash.setLineWidth(1.3);
        slash.stroke();
    }
}
