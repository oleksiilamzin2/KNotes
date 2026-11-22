//! How the note's text and the chat's are set (the settings' «Текст» tab): each one's font,
//! size and line height, read from the app's settings once and kept.

use std::cell::RefCell;

use knotes_session::{AppSettings, TextLook};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSParagraphStyle, NSParagraphStyleAttributeName, NSTextView};
use objc2_foundation::{NSRange, NSString};

/// The size of a note's text when none is set.
pub const NOTE_SIZE: f64 = 15.0;

thread_local! {
    /// The note's and the chat's.
    static SET: RefCell<Option<(TextLook, TextLook)>> = const { RefCell::new(None) };
}

fn both() -> (TextLook, TextLook) {
    SET.with(|set| {
        set.borrow_mut()
            .get_or_insert_with(|| {
                let settings = AppSettings::load(&knotes_session::app_data_dir());
                (settings.note_text, settings.chat_text)
            })
            .clone()
    })
}

pub fn note() -> TextLook {
    both().0
}

pub fn chat() -> TextLook {
    both().1
}

/// The settings were changed: these are the note's and the chat's from now on.
pub fn set(note: TextLook, chat: TextLook) {
    SET.with(|set| *set.borrow_mut() = Some((note, chat)));
}

/// The size of the chat's text when none is set: a point over the system's.
pub fn chat_size() -> f64 {
    NSFont::systemFontSize() + 1.0
}

/// `look`'s size in points; `own` when none is set.
pub fn size(look: &TextLook, own: f64) -> f64 {
    look.size.map_or(own, f64::from)
}

/// The height of a line of `font` as the font alone makes it.
pub fn font_height(font: &NSFont) -> f64 {
    font.ascender() - font.descender() + font.leading()
}

/// The room `look` asks for between lines of `font`, in points: its line height (in the
/// font's own heights) over the one the font has.
pub fn spacing(look: &TextLook, font: &NSFont) -> f64 {
    f64::from(look.line.unwrap_or(100).saturating_sub(100)) / 100.0 * font_height(font)
}

/// `look`'s font at `size`, bold when asked: the system's when no family is set, or the one
/// set is not on this Mac.
pub fn font(look: &TextLook, size: f64, bold: bool) -> Retained<NSFont> {
    let system = || if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    let Some(family) = &look.font else { return system() };
    let mtm = MainThreadMarker::new().expect("fonts are made on the main thread");
    let traits = if bold { NSFontTraitMask::BoldFontMask } else { NSFontTraitMask(0) };
    // Weight 5 is a family's regular face; the bold trait asks for 9 and over.
    NSFontManager::sharedFontManager(mtm).fontWithFamily_traits_weight_size(&NSString::from_str(family), traits, 5, size).unwrap_or_else(system)
}

/// `font` slanted, when its family has such a face.
pub fn italic(font: &NSFont) -> Retained<NSFont> {
    let mtm = MainThreadMarker::new().expect("fonts are made on the main thread");
    NSFontManager::sharedFontManager(mtm).convertFont_toHaveTrait(font, NSFontTraitMask::ItalicFontMask)
}

/// Probe: the family, the size and the room between lines `view`'s text starts with (an empty
/// view's: what is typed into it), as `Georgia 18 +9.0`.
pub fn describe(view: &NSTextView) -> String {
    // SAFETY: the view's own storage, read on the main thread.
    let storage = unsafe { view.textStorage() }.filter(|s| s.length() > 0);
    let at = |key: &NSString| {
        let mut range = NSRange::new(0, 0);
        // SAFETY: index 0 of a storage that is not empty, and an out pointer for the range.
        storage.as_ref().and_then(|s| unsafe { s.attribute_atIndex_effectiveRange(key, 0, &mut range) })
    };
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let (font, paragraph) = unsafe { (at(NSFontAttributeName), at(NSParagraphStyleAttributeName)) };
    let font = font.and_then(|f| f.downcast::<NSFont>().ok()).or_else(|| view.font());
    let paragraph = paragraph.and_then(|p| p.downcast::<NSParagraphStyle>().ok()).or_else(|| view.defaultParagraphStyle());
    let Some(font) = font else { return String::new() };
    format!("{} {} +{:.1}", font.familyName().map(|n| n.to_string()).unwrap_or_default(), font.pointSize(), paragraph.map_or(0.0, |p| p.lineSpacing()))
}
