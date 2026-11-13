//! The note column's text view (live preview editing, ADR 0005, 0011): a TextKit 2
//! `NSTextView` that lets the live preview take clicks on links first (ADR 0013), and shows a
//! hand over what a click acts on, and that gives pasted or dropped images to the window
//! (ADR 0020).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use objc2::rc::Retained;

use crate::quiet_scroll_view::QuietScrollView;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSCursor, NSDraggingInfo, NSEvent, NSEventModifierFlags, NSFont, NSPasteboard, NSResponder, NSScrollView, NSText,
    NSTextInputClient, NSTextView,
    NSControl, NSMenuItem, NSTextFinderAction, NSTrackingArea, NSTrackingAreaOptions, NSValidatedUserInterfaceItem, NSView,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize};
use objc2_quick_look_ui::QLPreviewPanel;

/// Gets the text view, the character index under a click and whether Cmd is held; returns
/// true when it handled the click (the text view then ignores it).
type OnClick = Box<dyn Fn(&NSTextView, usize, bool) -> bool>;
/// Whether a plain click on a character acts (the pointer shows a hand there).
type OnHover = Box<dyn Fn(usize) -> bool>;
/// Gets the find action Edit → Find… and its neighbours ask for.
type OnFind = Box<dyn Fn(isize)>;
/// Gets the pasteboard of a paste, or of a drop and the character index it falls at; true
/// when it took what is there (the text view then pastes nothing).
type OnPaste = Box<dyn Fn(&NSPasteboard, Option<usize>) -> bool>;
/// Gets the text view, the character where a resized image's line starts, the width it is
/// dragged to and whether the drag is over.
type OnResize = Box<dyn Fn(&NSTextView, usize, f64, bool)>;
/// Gets the text view and the character where an image's line starts; gives its file.
type OnImageFile = Box<dyn Fn(&NSTextView, usize) -> Option<PathBuf>>;

/// How near an image's right edge (points) the pointer resizes it.
const IMAGE_EDGE: f64 = 6.0;

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
    on_paste: RefCell<Option<OnPaste>>,
    on_resize: RefCell<Option<OnResize>>,
    on_image_file: RefCell<Option<OnImageFile>>,
    /// The image the last click was on (where its line starts): a second click previews it,
    /// though the first one moved it below its source.
    image_clicked: Cell<Option<usize>>,
    /// The file the Quick Look panel shows while this view controls it.
    quick_look: RefCell<Option<Retained<crate::quick_look::QuickLookSource>>>,
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
            if self.image_edge(event.locationInWindow()).is_some() {
                resize_cursor().set();
            } else if self.over_clickable(event.locationInWindow()) {
                NSCursor::pointingHandCursor().set();
            }
        }

        /// The text view would set the I-beam over a link too.
        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, event: &NSEvent) {
            if self.image_edge(event.locationInWindow()).is_some() {
                resize_cursor().set();
            } else if self.over_clickable(event.locationInWindow()) {
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

        #[unsafe(method(paste:))]
        fn paste(&self, sender: Option<&AnyObject>) {
            if !self.offer_paste(&NSPasteboard::generalPasteboard(), None) {
                // SAFETY: forwarding the same action to the superclass implementation.
                let _: () = unsafe { msg_send![super(self), paste: sender] };
            }
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let point = self.convertPoint_fromView(info.draggingLocation(), None);
            let index = self.characterIndexForInsertionAtPoint(point);
            // SAFETY: forwarding the same drag to the superclass implementation.
            self.offer_paste(&info.draggingPasteboard(), Some(index)) || unsafe { msg_send![super(self), performDragOperation: info] }
        }

        /// The Quick Look panel looks for its controller along the responder chain.
        #[unsafe(method(acceptsPreviewPanelControl:))]
        fn accepts_preview_panel_control(&self, _panel: Option<&QLPreviewPanel>) -> bool {
            self.ivars().quick_look.borrow().is_some()
        }

        #[unsafe(method(beginPreviewPanelControl:))]
        fn begin_preview_panel_control(&self, panel: Option<&QLPreviewPanel>) {
            if let (Some(panel), Some(source)) = (panel, self.ivars().quick_look.borrow().as_ref()) {
                // SAFETY: the source lives while this view controls the panel.
                unsafe {
                    panel.setDataSource(Some(ProtocolObject::from_ref(&**source)));
                    panel.reloadData();
                }
            }
        }

        #[unsafe(method(endPreviewPanelControl:))]
        fn end_preview_panel_control(&self, panel: Option<&QLPreviewPanel>) {
            if let Some(panel) = panel {
                // SAFETY: clearing the source before it goes.
                unsafe { panel.setDataSource(None) };
            }
            self.ivars().quick_look.replace(None);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let clicked = self.ivars().image_clicked.take();
            if event.clickCount() >= 2 && let Some(line) = clicked {
                self.quick_look(line);
                return;
            }
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            self.ivars().image_clicked.set(crate::preview_fragment::image_at(self, point).map(|(line, _, _)| line));
            if let Some((line, rect)) = self.image_edge(event.locationInWindow()) {
                self.resize_image(line, rect);
            } else if !self.offer_click(event) {
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

    pub fn on_resize(&self, callback: impl Fn(&NSTextView, usize, f64, bool) + 'static) {
        *self.ivars().on_resize.borrow_mut() = Some(Box::new(callback));
    }

    /// The image whose right edge is at a point in window coordinates: where its line starts
    /// and its rect in the view.
    fn image_edge(&self, in_window: NSPoint) -> Option<(usize, NSRect)> {
        self.image_edge_at(self.convertPoint_fromView(in_window, None))
    }

    pub fn on_image_file(&self, callback: impl Fn(&NSTextView, usize) -> Option<PathBuf> + 'static) {
        *self.ivars().on_image_file.borrow_mut() = Some(Box::new(callback));
    }

    /// The file of the image on the line starting at `line`.
    pub fn image_file(&self, line: usize) -> Option<PathBuf> {
        self.ivars().on_image_file.borrow().as_ref()?(self, line)
    }

    /// Shows the image on the line starting at `line` in the Quick Look panel.
    fn quick_look(&self, line: usize) {
        let Some(file) = self.image_file(line) else { return };
        self.ivars().quick_look.replace(Some(crate::quick_look::QuickLookSource::new(self.mtm(), &file)));
        // SAFETY: the shared panel of this app, on the main thread.
        let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(self.mtm()) }) else { return };
        if panel.isVisible() {
            // SAFETY: as above.
            unsafe {
                panel.updateController();
                panel.reloadData();
            }
        } else {
            panel.makeKeyAndOrderFront(None);
        }
    }

    /// The same for a point in the view's coordinates.
    pub fn image_edge_at(&self, point: NSPoint) -> Option<(usize, NSRect)> {
        let (line, rect, below) = crate::preview_fragment::image_at(self, point)?;
        let right = rect.origin.x + rect.size.width;
        let inside = point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height;
        (!below && inside && (point.x - right).abs() <= IMAGE_EDGE).then_some((line, rect))
    }

    /// Follows the mouse until it goes up: the centred image grows or shrinks on both sides,
    /// its right edge under the pointer.
    fn resize_image(&self, line: usize, rect: NSRect) {
        let (Some(window), Some(callback)) = (self.window(), self.ivars().on_resize.borrow_mut().take()) else { return };
        let centre = rect.origin.x + rect.size.width / 2.0;
        while let Some(event) = window.nextEventMatchingMask(objc2_app_kit::NSEventMask::LeftMouseDragged | objc2_app_kit::NSEventMask::LeftMouseUp) {
            let x = self.convertPoint_fromView(event.locationInWindow(), None).x;
            let done = event.r#type() == objc2_app_kit::NSEventType::LeftMouseUp;
            callback(self, line, 2.0 * (x - centre), done);
            if done {
                break;
            }
        }
        *self.ivars().on_resize.borrow_mut() = Some(callback);
    }

    pub fn on_paste(&self, callback: impl Fn(&NSPasteboard, Option<usize>) -> bool + 'static) {
        *self.ivars().on_paste.borrow_mut() = Some(Box::new(callback));
    }

    /// Offers a paste or a drop (at character `at`) to the window first.
    pub fn offer_paste(&self, pasteboard: &NSPasteboard, at: Option<usize>) -> bool {
        self.ivars().on_paste.borrow().as_ref().is_some_and(|callback| callback(pasteboard, at))
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

/// The pointer over an image's right edge: a frame's right side being resized.
fn resize_cursor() -> Retained<NSCursor> {
    NSCursor::frameResizeCursorFromPosition_inDirections(objc2_app_kit::NSCursorFrameResizePosition::Right, objc2_app_kit::NSCursorFrameResizeDirections::All)
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
