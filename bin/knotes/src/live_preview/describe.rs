//! What the note shows, in words, for the probe.

use super::*;

impl LivePreview {
    /// Probe lines of what is drawn in place of hidden source in view.
    pub fn describe_drawn(view: &NSTextView) -> Vec<String> {
        crate::preview_fragment::describe_visible(view)
    }

    /// The struck pieces of the text, `|` between those apart, for probe mode.
    pub fn describe_struck(view: &NSTextView) -> String {
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        Self::describe_with(view, unsafe { NSStrikethroughStyleAttributeName }, |v| v.downcast_ref::<NSNumber>().is_some_and(|n| n.integerValue() != 0))
    }

    /// The pieces of text with a background (inline code), `|` between those apart.
    pub fn describe_shaded(view: &NSTextView) -> String {
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        Self::describe_with(view, unsafe { NSBackgroundColorAttributeName }, |_| true)
    }

    /// The pieces of text whose `key` attribute `has` holds for, `|` between those apart.
    pub(super) fn describe_with(view: &NSTextView, key: &NSString, has: impl Fn(&AnyObject) -> bool) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let utf16: Vec<u16> = storage.string().to_string().encode_utf16().collect();
        let (mut pieces, mut at, mut last_end) = (String::new(), 0, None);
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            // SAFETY: a valid index and an out pointer for the range.
            let value = unsafe { storage.attribute_atIndex_effectiveRange(key, at, &mut range) };
            let end = (range.location + range.length).max(at + 1);
            if value.is_some_and(|v| has(&v)) {
                if last_end.is_some_and(|e| e != at) {
                    pieces.push('|');
                }
                pieces.push_str(&String::from_utf16_lossy(&utf16[at..end]));
                last_end = Some(end);
            }
            at = end;
        }
        pieces
    }

    /// `[style]text|[style]text…` of the text view's storage, for probe mode.
    pub fn describe(view: &NSTextView) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let key = NSString::from_str(STYLE_KEY);
        let text = storage.string().to_string();
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let mut parts: Vec<(String, String)> = Vec::new();
        let mut at = 0;
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            let value = unsafe { storage.attribute_atIndex_effectiveRange(&key, at, &mut range) };
            let name = value.and_then(|v| v.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default();
            let end = (range.location + range.length).max(at + 1);
            let piece = String::from_utf16_lossy(&utf16[at..end]).replace('\n', "⏎");
            // Other attributes (kerning) split effective ranges: merge runs of one style.
            match parts.last_mut() {
                Some((last, text)) if *last == name => text.push_str(&piece),
                _ => parts.push((name, piece)),
            }
            at = end;
        }
        parts.iter().map(|(name, piece)| format!("[{name}]{piece}")).collect::<Vec<_>>().join("|")
    }
}
