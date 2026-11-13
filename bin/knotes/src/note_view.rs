//! The note column's text view (live preview editing, ADR 0005, 0011): a TextKit 2
//! `NSTextView` that lets the live preview take clicks on links first (ADR 0013), and shows a
//! hand over what a click acts on.

use std::cell::RefCell;

use objc2::rc::Retained;

use crate::quiet_scroll_view::QuietScrollView;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSCursor, NSEvent, NSEventModifierFlags, NSFont, NSResponder, NSScrollView, NSText, NSTextInputClient, NSTextView,
    NSControl, NSMenuItem, NSTextFinderAction, NSTrackingArea, NSTrackingAreaOptions, NSValidatedUserInterfaceItem, NSView,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize};

/// Gets the text view, the character index under a click and whether Cmd is held; returns
/// true when it handled the click (the text view then ignores it).
type OnClick = Box<dyn Fn(&NSTextView, usize, bool) -> bool>;
/// Whether a plain click on a character acts (the pointer shows a hand there).
type OnHover = Box<dyn Fn(usize) -> bool>;
/// Gets the find action Edit → Find… and its neighbours ask for.
type OnFind = Box<dyn Fn(isize)>;

#[derive(Default)]
pub struct Ivars {
    on_click: RefCell<Option<OnClick>>,
    on_hover: RefCell<Option<OnHover>>,
    /// Called when the view turns light or dark.
    on_appearance: RefCell<Option<Box<dyn Fn()>>>,
    /// The tracking area that reports mouse moves over the view.
    moves: RefCell<Option<Retained<NSTrackingArea>>>,
    /// Called with the find action (`NSTextFinderAction`) Edit → Find… and its neighbours ask
    /// for: the window's own find bar takes them.
    on_find: RefCell<Option<OnFind>>,
}

define_class!(
    #[unsafe(super(NSTextView, NSText, NSView, NSResponder, objc2_foundation::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesNoteTextView"]
    #[ivars = Ivars]
    pub struct NoteTextView;

    unsafe impl NSObjectProtocol for NoteTextView {}


    impl NoteTextView {
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // SAFETY: the superclass keeps its own areas up to date.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
            if self.ivars().moves.borrow().is_none() {
                let options = NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::ActiveInKeyWindow | NSTrackingAreaOptions::InVisibleRect;
                // SAFETY: the view owns the area and outlives it.
                let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(NSTrackingArea::alloc(), NSRect::ZERO, options, Some(self), None) };
                self.addTrackingArea(&area);
                *self.ivars().moves.borrow_mut() = Some(area);
            }
        }

        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            // SAFETY: the superclass updates its own colours first.
            let _: () = unsafe { msg_send![super(self), viewDidChangeEffectiveAppearance] };
            if let Some(callback) = self.ivars().on_appearance.borrow().as_ref() {
                callback();
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            // SAFETY: forwarding the same event to the superclass implementation.
            let _: () = unsafe { msg_send![super(self), mouseMoved: event] };
            if self.over_clickable(event.locationInWindow()) {
                NSCursor::pointingHandCursor().set();
            }
        }

        /// The text view would set the I-beam over a link too.
        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, event: &NSEvent) {
            if self.over_clickable(event.locationInWindow()) {
                NSCursor::pointingHandCursor().set();
            } else {
                // SAFETY: forwarding the same event to the superclass implementation.
                let _: () = unsafe { msg_send![super(self), cursorUpdate: event] };
            }
        }

        /// Edit → Find… and its neighbours go to the window's find bar.
        #[unsafe(method(performTextFinderAction:))]
        fn perform_text_finder_action(&self, sender: Option<&AnyObject>) {
            if let Some(callback) = self.ivars().on_find.borrow().as_ref() {
                callback(finder_tag(sender));
            }
        }

        /// The find bar is the window's: the text view's own find items would be off.
        #[unsafe(method(validateUserInterfaceItem:))]
        fn validate_item(&self, item: &ProtocolObject<dyn NSValidatedUserInterfaceItem>) -> bool {
            if item.action() == Some(sel!(performTextFinderAction:)) {
                true
            } else {
                // SAFETY: the superclass validates its other actions.
                unsafe { msg_send![super(self), validateUserInterfaceItem: item] }
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if !self.offer_click(event) {
                // SAFETY: forwarding the same event to the superclass implementation.
                let _: () = unsafe { msg_send![super(self), mouseDown: event] };
            }
        }
    }
);

/// The text finder action a menu item or control asks for (its tag).
pub fn finder_tag(sender: Option<&AnyObject>) -> isize {
    let Some(sender) = sender else { return NSTextFinderAction::ShowFindInterface.0 };
    if let Some(item) = sender.downcast_ref::<NSMenuItem>() {
        item.tag()
    } else if let Some(control) = sender.downcast_ref::<NSControl>() {
        control.tag()
    } else {
        NSTextFinderAction::ShowFindInterface.0
    }
}

impl NoteTextView {
    pub fn on_find(&self, callback: impl Fn(isize) + 'static) {
        *self.ivars().on_find.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_click(&self, callback: impl Fn(&NSTextView, usize, bool) -> bool + 'static) {
        *self.ivars().on_click.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_appearance(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_appearance.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_hover(&self, callback: impl Fn(usize) -> bool + 'static) {
        *self.ivars().on_hover.borrow_mut() = Some(Box::new(callback));
    }

    /// Whether the point (window coordinates) is over something a plain click acts on.
    fn over_clickable(&self, in_window: NSPoint) -> bool {
        let hover = self.ivars().on_hover.borrow();
        hover.as_ref().zip(self.character_under(in_window)).is_some_and(|(hover, index)| hover(index))
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
