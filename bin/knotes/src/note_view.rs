//! The note column's text view (live preview editing, ADR 0005, 0011): a TextKit 2
//! `NSTextView` that lets the live preview take clicks on links first (ADR 0013).

use std::cell::RefCell;

use objc2::rc::Retained;

use crate::quiet_scroll_view::QuietScrollView;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSEvent, NSEventModifierFlags, NSFont, NSResponder, NSScrollView, NSText, NSTextInputClient, NSTextView, NSView};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize};

/// Gets the text view, the character index under a click and whether Cmd is held; returns
/// true when it handled the click (the text view then ignores it).
type OnClick = Box<dyn Fn(&NSTextView, usize, bool) -> bool>;

#[derive(Default)]
pub struct Ivars {
    on_click: RefCell<Option<OnClick>>,
}

define_class!(
    #[unsafe(super(NSTextView, NSText, NSView, NSResponder, objc2_foundation::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesNoteTextView"]
    #[ivars = Ivars]
    pub struct NoteTextView;

    unsafe impl NSObjectProtocol for NoteTextView {}

    impl NoteTextView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if !self.offer_click(event) {
                // SAFETY: forwarding the same event to the superclass implementation.
                let _: () = unsafe { msg_send![super(self), mouseDown: event] };
            }
        }
    }
);

impl NoteTextView {
    pub fn on_click(&self, callback: impl Fn(&NSTextView, usize, bool) -> bool + 'static) {
        *self.ivars().on_click.borrow_mut() = Some(Box::new(callback));
    }

    fn offer_click(&self, event: &NSEvent) -> bool {
        let callback = self.ivars().on_click.borrow();
        let Some(callback) = callback.as_ref() else { return false };
        let Some(index) = self.character_under(event.locationInWindow()) else { return false };
        callback(self, index, event.modifierFlags().contains(NSEventModifierFlags::Command))
    }

    /// The character whose glyph is under a point in window coordinates: the insertion index
    /// or the one before it, whichever box holds the point (not just the nearest gap).
    fn character_under(&self, in_window: NSPoint) -> Option<usize> {
        let point = self.convertPoint_fromView(in_window, None);
        let window = self.window()?;
        let on_screen = window.convertPointToScreen(in_window);
        let index = self.characterIndexForInsertionAtPoint(point);
        let length = self.string().length();
        [index, index.wrapping_sub(1)].into_iter().filter(|&i| i < length).find(|&i| {
            // SAFETY: the range lies inside the text; a null actual range is allowed.
            let rect = unsafe { self.firstRectForCharacterRange_actualRange(NSRange::new(i, 1), std::ptr::null_mut()) };
            contains(rect, on_screen)
        })
    }
}

impl NoteTextView {
    /// Puts the cursor at `index` and scrolls its line to the top of the column.
    pub fn move_to_top(&self, index: usize) {
        let index = index.min(self.string().length());
        self.setSelectedRange(NSRange::new(index, 0));
        let Some(window) = self.window() else { return };
        // SAFETY: the range lies inside the text; a null actual range is allowed.
        let on_screen = unsafe { self.firstRectForCharacterRange_actualRange(NSRange::new(index, 0), std::ptr::null_mut()) };
        let rect = self.convertRect_fromView(window.convertRectFromScreen(on_screen), None);
        self.scrollPoint(NSPoint::new(0.0, (rect.origin.y - self.textContainerInset().height).max(0.0)));
    }
}

fn contains(rect: NSRect, p: NSPoint) -> bool {
    p.x >= rect.origin.x && p.x <= rect.origin.x + rect.size.width && p.y >= rect.origin.y && p.y <= rect.origin.y + rect.size.height
}

pub fn note_view(mtm: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NoteTextView>) {
    // Keeps the toolbar's glass without its separator line (ADR 0016).
    let scroll: Retained<NSScrollView> = Retained::into_super(QuietScrollView::new(mtm));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    let size = scroll.contentSize();
    let this = NoteTextView::alloc(mtm).set_ivars(Ivars::default());
    // SAFETY: NSTextView's designated initializer; TextKit 2 (ADR 0012).
    let text: Retained<NoteTextView> = unsafe { msg_send![super(this), initUsingTextLayoutManager: true] };
    text.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
    text.setMinSize(NSSize::new(0.0, size.height));
    text.setMaxSize(NSSize::new(f64::MAX, f64::MAX));
    text.setVerticallyResizable(true);
    text.setHorizontallyResizable(false);
    text.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    if let Some(container) = unsafe { text.textContainer() } {
        container.setWidthTracksTextView(true);
        container.setContainerSize(NSSize::new(size.width, f64::MAX));
    }
    text.setEditable(true);
    text.setSelectable(true);
    text.setRichText(false);
    text.setAllowsUndo(true);
    // Markdown is plain text: no curly quotes, dashes or replacements behind the user's back.
    text.setAutomaticQuoteSubstitutionEnabled(false);
    text.setAutomaticDashSubstitutionEnabled(false);
    text.setAutomaticTextReplacementEnabled(false);
    text.setContinuousSpellCheckingEnabled(true);
    text.setFont(Some(&NSFont::systemFontOfSize(15.0)));
    scroll.setDocumentView(Some(&text));
    (scroll, text)
}
