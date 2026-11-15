//! How a space looks (ADR 0030): its colour as one of macOS's system colours, and its dot.

use objc2::rc::Retained;
use objc2_app_kit::{NSColor, NSImage, NSImageSymbolConfiguration};
use objc2_foundation::{NSArray, NSString};

use knotes_spaces::SpaceColor;

/// The system colour a space's colour names (it follows light and dark appearance).
pub fn color(color: SpaceColor) -> Retained<NSColor> {
    match color {
        SpaceColor::Blue => NSColor::systemBlueColor(),
        SpaceColor::Purple => NSColor::systemPurpleColor(),
        SpaceColor::Pink => NSColor::systemPinkColor(),
        SpaceColor::Red => NSColor::systemRedColor(),
        SpaceColor::Orange => NSColor::systemOrangeColor(),
        SpaceColor::Yellow => NSColor::systemYellowColor(),
        SpaceColor::Green => NSColor::systemGreenColor(),
        SpaceColor::Teal => NSColor::systemTealColor(),
        SpaceColor::Gray => NSColor::systemGrayColor(),
        // One's own, as chosen: the same in light and dark appearance.
        SpaceColor::Custom(r, g, b) => NSColor::colorWithSRGBRed_green_blue_alpha(f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0, 1.0),
    }
}

/// A space's dot: a filled circle of its colour, `size` points across, described for
/// VoiceOver as `description`.
pub fn dot(of: SpaceColor, size: f64, description: &str) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("circle.fill"), Some(&NSString::from_str(description)))?;
    let palette = NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&[color(of)]));
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(size, 0.0).configurationByApplyingConfiguration(&palette);
    image.imageWithSymbolConfiguration(&config)
}
