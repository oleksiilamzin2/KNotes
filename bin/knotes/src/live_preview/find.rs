//! Words found (a search, Cmd+F) highlighted in the note.

use super::*;

impl LivePreview {
    /// Highlights `ranges` (UTF-16), the words the search found in the note, until the next
    /// edit or note; the first is scrolled into view with the find indicator.
    pub fn show_found(&self, view: &NSTextView, ranges: &[(usize, usize)]) {
        self.highlight(view, ranges);
        let Some(&first) = ranges.first() else { return };
        let first = NSRange::new(first.0, first.1 - first.0);
        view.scrollRangeToVisible(first);
        view.showFindIndicatorForRange(first);
    }

    /// Highlights `ranges` (UTF-16) in the find colour, instead of what was highlighted, until
    /// the next edit or note.
    pub fn highlight(&self, view: &NSTextView, ranges: &[(usize, usize)]) {
        self.clear_found(view);
        let Some(layout) = view.textLayoutManager() else { return };
        let Some(content) = layout.textContentManager() else { return };
        let start = content.documentRange().location();
        let color = NSColor::findHighlightColor();
        for &(from, to) in ranges {
            let (Some(a), Some(b)) =
                (content.locationFromLocation_withOffset(&start, from as isize), content.locationFromLocation_withOffset(&start, to as isize))
            else {
                continue;
            };
            if let Some(range) = NSTextRange::initWithLocation_endLocation(NSTextRange::alloc(), &a, Some(&b)) {
                // SAFETY: a colour is the background attribute's value type.
                unsafe { layout.addRenderingAttribute_value_forTextRange(NSBackgroundColorAttributeName, Some(&color), &range) };
            }
        }
        self.ivars().found.set(ranges.len());
    }

    /// Drops the highlights of found words.
    pub fn clear_found(&self, view: &NSTextView) {
        if self.ivars().found.replace(0) == 0 {
            return;
        }
        if let (Some(layout), Some(content)) = (view.textLayoutManager(), view.textLayoutManager().and_then(|l| l.textContentManager())) {
            // SAFETY: AppKit's attribute name constant lives for the whole process.
            layout.removeRenderingAttribute_forTextRange(unsafe { NSBackgroundColorAttributeName }, &content.documentRange());
        }
    }

    /// How many found words the note shows highlighted (probe mode).
    pub fn found_count(&self) -> usize {
        self.ivars().found.get()
    }
}
