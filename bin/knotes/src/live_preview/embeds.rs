//! Embedded notes (ADR 0034): the room their boxes keep, where their views go, the boxes drawn.

use super::*;

impl LivePreview {
    /// Styles a note embedded in another, in `view` (ADR 0034): as `attach`, without the centred
    /// column or a scroll view to follow.
    pub fn attach_embedded(&self, view: &NSTextView) {
        self.ivars().embedded.set(true);
        *self.ivars().undo.borrow_mut() = Some(NSUndoManager::new(MainThreadMarker::from(view)));
        view.setPostsFrameChangedNotifications(true);
        let observer: &AnyObject = self.as_ref();
        // SAFETY: the observer lives as long as the view (both kept together by the window), and
        // the selector takes one notification argument.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                observer,
                sel!(frameDidChange:),
                Some(NSViewFrameDidChangeNotification),
                Some(view.as_ref()),
            );
        }
        self.ivars().column.set(text_column(view));
        *self.ivars().view.borrow_mut() = objc2::rc::Weak::new(view);
        if let Some(layout) = view.textLayoutManager() {
            layout.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// The notes this one embeds alone on their lines (ADR 0034): each one's key (its source,
    /// and how many the same came before it, so it stays while text above changes), its link
    /// and its range (UTF-16).
    pub fn embeds(&self, text: &NSString) -> Vec<(String, Link, (usize, usize))> {
        let styled = self.ivars().styled.borrow();
        let mut seen: HashMap<String, usize> = HashMap::new();
        styled
            .embeds
            .iter()
            .filter(|e| e.range.end <= text.length())
            .map(|e| {
                let source = text.substringWithRange(ns(&e.range)).to_string();
                let nth = seen.entry(source.clone()).or_default();
                let key = format!("{source}\u{1}{nth}");
                *nth += 1;
                (key, e.link.clone(), (e.range.start, e.range.end))
            })
            .collect()
    }

    /// The box height of each embedded note shown (by key); the lines of those whose height
    /// changed keep their new room at once.
    pub fn set_embed_heights(&self, view: &NSTextView, heights: HashMap<String, f64>) {
        let old = self.ivars().embed_heights.replace(heights);
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let changed: Vec<(usize, usize)> = self
            .embeds(&storage.string())
            .into_iter()
            .filter(|(key, _, _)| old.get(key) != self.ivars().embed_heights.borrow().get(key))
            .map(|(_, _, range)| paragraph(&storage, NSRange::new(range.0, range.1 - range.0)))
            .collect();
        for para in changed {
            self.restyle(&storage, para);
        }
    }

    /// Where each embedded note's box is in `view` (by key), as laid out now.
    pub fn embed_frames(&self, view: &NSTextView) -> Vec<(String, NSRect)> {
        let (Some(layout), Some(storage)) = (view.textLayoutManager(), unsafe { view.textStorage() }) else { return Vec::new() };
        let Some(content) = layout.textContentManager() else { return Vec::new() };
        let start = content.documentRange().location();
        let origin = view.textContainerOrigin();
        let column = unsafe { view.textContainer() }.map_or(0.0, |c| c.size().width);
        let revealed = self.ivars().revealed.get();
        let heights = self.ivars().embed_heights.borrow().clone();
        self.embeds(&storage.string())
            .into_iter()
            .filter_map(|(key, _, range)| {
                let height = *heights.get(&key)?;
                let para = paragraph(&storage, NSRange::new(range.0, range.1 - range.0));
                let location = content.locationFromLocation_withOffset(&start, para.0 as isize)?;
                let fragment = layout.textLayoutFragmentForLocation(&location)?;
                let frame = fragment.layoutFragmentFrame();
                let top = if overlaps(&(range.0..range.1), revealed) {
                    // Not laid out yet (mid-edit): placed after the layout to come.
                    first_line_bottom(&fragment)? + PREVIEW_GAP
                } else {
                    PREVIEW_GAP
                };
                Some((key, NSRect::new(NSPoint::new(origin.x, origin.y + frame.origin.y + top), NSSize::new(column, height))))
            })
            .collect()
    }

    /// An embedded note's line (ADR 0034): its source hidden and the room of its box kept, or,
    /// while the cursor is on it, the source shown with the box's room below it.
    pub(super) fn paint_embed<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, range: (usize, usize), height: f64, revealed: bool) {
        let line = paragraph(storage, NSRange::new(range.0, range.1 - range.0));
        let reserved = height + 2.0 * PREVIEW_GAP;
        if revealed {
            let para = palette.para(Para { spacing_after: reserved, ..Para::default() });
            canvas.paint(line.0..line.1, |p| p.para = para);
            return;
        }
        let look = &self.ivars().look;
        let text = storage.string();
        let end = if line.1 > line.0 && matches!(text.characterAtIndex(line.1 - 1), 0x0a | 0x0d) { line.1 - 1 } else { line.1 };
        let hidden = palette.look(&look.hidden, &look.clear, "embed");
        canvas.paint(line.0..end, |p| {
            p.look = hidden;
            p.kern = 0;
        });
        let para = palette.para(Para { min_line: reserved, max_line: reserved, ..Para::default() });
        canvas.paint(line.0..line.1, |p| p.para = para);
    }
}
