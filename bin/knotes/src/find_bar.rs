//! Find in the open note (Cmd+F): a bar over the note with the text to find, which occurrence
//! of how many, back and forward, and Done. Every occurrence is highlighted and the current one
//! selected; the note is not dimmed (the owner's choice over the system find bar, which dims it
//! and did not open for the owner).

use std::cell::RefCell;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSButton, NSColor, NSControl, NSControlSize, NSControlTextEditingDelegate, NSEventModifierFlags,
    NSFont, NSImage, NSLayoutConstraint, NSScrollView, NSSearchField, NSSearchFieldDelegate, NSSegmentSwitchTracking, NSSegmentedControl, NSTextField,
    NSTextFieldDelegate, NSTextView, NSView,
};
use objc2_foundation::{NSArray, NSEdgeInsets, NSNotification, NSObject, NSObjectProtocol, NSString};

use crate::strings;

/// Height of the bar.
const BAR_HEIGHT: f64 = 40.0;
const SIDE_PADDING: f64 = 12.0;

type OnText = Box<dyn Fn(String)>;
type OnStep = Box<dyn Fn(bool)>;
type OnClose = Box<dyn Fn()>;

#[derive(Default)]
pub struct ControlsIvars {
    on_text: RefCell<Option<OnText>>,
    on_step: RefCell<Option<OnStep>>,
    on_close: RefCell<Option<OnClose>>,
}

define_class!(
    /// The bar's controls' target and the field's delegate.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesFindControls"]
    #[ivars = ControlsIvars]
    struct FindControls;

    unsafe impl NSObjectProtocol for FindControls {}

    unsafe impl NSControlTextEditingDelegate for FindControls {
        /// Every keystroke finds again.
        #[unsafe(method(controlTextDidChange:))]
        fn text_did_change(&self, notification: &NSNotification) {
            let Some(field) = notification.object().and_then(|o| o.downcast::<NSSearchField>().ok()) else { return };
            if let Some(callback) = self.ivars().on_text.borrow().as_ref() {
                callback(field.stringValue().to_string());
            }
        }

        /// Return: the next occurrence (Shift+Return: the one before); Escape: Done.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, _control: &NSControl, _view: &NSTextView, command: Sel) -> bool {
            if command == sel!(insertNewline:) || command == sel!(insertLineBreak:) || command == sel!(insertNewlineIgnoringFieldEditor:) {
                let shift = NSApplication::sharedApplication(self.mtm())
                    .currentEvent()
                    .is_some_and(|e| e.modifierFlags().contains(NSEventModifierFlags::Shift));
                self.step(!shift);
                true
            } else if command == sel!(cancelOperation:) {
                self.close();
                true
            } else {
                false
            }
        }
    }

    unsafe impl NSTextFieldDelegate for FindControls {}

    unsafe impl NSSearchFieldDelegate for FindControls {}

    impl FindControls {
        #[unsafe(method(findStep:))]
        fn find_step(&self, sender: &NSSegmentedControl) {
            self.step(sender.selectedSegment() == 1);
        }

        #[unsafe(method(findDone:))]
        fn find_done(&self, _sender: Option<&AnyObject>) {
            self.close();
        }
    }
);

impl FindControls {
    fn step(&self, forward: bool) {
        if let Some(callback) = self.ivars().on_step.borrow().as_ref() {
            callback(forward);
        }
    }

    fn close(&self) {
        if let Some(callback) = self.ivars().on_close.borrow().as_ref() {
            callback();
        }
    }
}

pub struct FindBar {
    /// The note column: the bar over the note's scroll view.
    pub column: Retained<NSView>,
    pub field: Retained<NSSearchField>,
    count: Retained<NSTextField>,
    bar: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    /// Another scroll view in the column (a PDF's), kept clear of the bar as the note's is.
    other: std::cell::RefCell<Option<Retained<NSScrollView>>>,
    controls: Retained<FindControls>,
}

impl FindBar {
    pub fn new(mtm: MainThreadMarker, scroll: Retained<NSScrollView>) -> Self {
        let column = NSView::new(mtm);
        scroll.setFrame(column.bounds());
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        column.addSubview(&scroll);
        let bar = NSView::new(mtm);
        bar.setTranslatesAutoresizingMaskIntoConstraints(false);
        bar.setHidden(true);
        bar.setWantsLayer(true);
        if let Some(layer) = bar.layer() {
            layer.setBackgroundColor(Some(&NSColor::windowBackgroundColor().CGColor()));
        }
        column.addSubview(&bar);
        let controls: Retained<FindControls> = unsafe { msg_send![super(FindControls::alloc(mtm).set_ivars(ControlsIvars::default())), init] };
        let field = NSSearchField::new(mtm);
        field.setPlaceholderString(Some(&NSString::from_str(strings::FIND_PLACEHOLDER)));
        if let Some(cell) = field.cell() {
            cell.setScrollable(true);
            cell.setWraps(false);
            cell.setUsesSingleLineMode(true);
        }
        // SAFETY: the controls live as long as the bar, which keeps them.
        unsafe { field.setDelegate(Some(ProtocolObject::from_ref(&*controls))) };
        field.setTranslatesAutoresizingMaskIntoConstraints(false);
        let count = NSTextField::labelWithString(&NSString::new(), mtm);
        count.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        count.setTextColor(Some(&NSColor::secondaryLabelColor()));
        count.setTranslatesAutoresizingMaskIntoConstraints(false);
        let images = [symbol("chevron.left", strings::FIND_PREVIOUS), symbol("chevron.right", strings::FIND_NEXT)];
        let images = NSArray::from_retained_slice(&images.into_iter().flatten().collect::<Vec<_>>());
        // SAFETY: the controls are the target and live as long as the bar.
        let arrows = unsafe {
            NSSegmentedControl::segmentedControlWithImages_trackingMode_target_action(
                &images,
                NSSegmentSwitchTracking::Momentary,
                Some(&controls),
                Some(sel!(findStep:)),
                mtm,
            )
        };
        arrows.setControlSize(NSControlSize::Small);
        arrows.setTranslatesAutoresizingMaskIntoConstraints(false);
        // SAFETY: as above.
        let done = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::FIND_DONE), Some(&controls), Some(sel!(findDone:)), mtm) };
        done.setControlSize(NSControlSize::Small);
        done.setTranslatesAutoresizingMaskIntoConstraints(false);
        for view in [field.as_ref() as &NSView, &count, &arrows, &done] {
            bar.addSubview(view);
        }
        let safe = column.safeAreaLayoutGuide();
        let constraints = [
            bar.topAnchor().constraintEqualToAnchor(&safe.topAnchor()),
            bar.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            bar.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            bar.heightAnchor().constraintEqualToConstant(BAR_HEIGHT),
            field.leadingAnchor().constraintEqualToAnchor_constant(&bar.leadingAnchor(), SIDE_PADDING),
            field.centerYAnchor().constraintEqualToAnchor(&bar.centerYAnchor()),
            count.leadingAnchor().constraintEqualToAnchor_constant(&field.trailingAnchor(), 8.0),
            count.centerYAnchor().constraintEqualToAnchor(&bar.centerYAnchor()),
            arrows.leadingAnchor().constraintEqualToAnchor_constant(&count.trailingAnchor(), 8.0),
            arrows.centerYAnchor().constraintEqualToAnchor(&bar.centerYAnchor()),
            done.leadingAnchor().constraintEqualToAnchor_constant(&arrows.trailingAnchor(), 8.0),
            done.trailingAnchor().constraintEqualToAnchor_constant(&bar.trailingAnchor(), -SIDE_PADDING),
            done.centerYAnchor().constraintEqualToAnchor(&bar.centerYAnchor()),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        // The count keeps its width; the field takes the rest.
        count.setContentCompressionResistancePriority_forOrientation(800.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
        count.setContentHuggingPriority_forOrientation(750.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
        Self { column, field, count, bar, scroll, other: std::cell::RefCell::new(None), controls }
    }

    /// `on_text` gets the text to find as it is typed, `on_step` `true` for the next occurrence
    /// and `false` for the one before, `on_close` Done or Escape.
    pub fn on_events(&self, on_text: impl Fn(String) + 'static, on_step: impl Fn(bool) + 'static, on_close: impl Fn() + 'static) {
        let ivars = self.controls.ivars();
        *ivars.on_text.borrow_mut() = Some(Box::new(on_text));
        *ivars.on_step.borrow_mut() = Some(Box::new(on_step));
        *ivars.on_close.borrow_mut() = Some(Box::new(on_close));
    }

    pub fn is_shown(&self) -> bool {
        !self.bar.isHidden()
    }

    /// Shows the bar with `text`, the focus left where it is (arrows through a search's results
    /// keep moving in the list).
    pub fn show_with(&self, text: &str) {
        self.bar.setHidden(false);
        self.inset();
        self.set_text(text);
    }

    /// Shows the bar with the focus in its field (its text selected), or hides it. The note gets
    /// the bar's height as extra top inset, so no text hides behind it.
    pub fn show(&self, shown: bool) {
        self.bar.setHidden(!shown);
        self.inset();
        if shown && let Some(window) = self.field.window() {
            window.makeFirstResponder(Some(&self.field));
            if let Some(editor) = self.field.currentEditor() {
                // SAFETY: the field's own editor, selecting its text.
                unsafe { editor.selectAll(None) };
            }
        }
    }

    /// The scroll views in the column get the bar's height as extra top inset while it shows.
    fn inset(&self) {
        let top = if self.is_shown() { BAR_HEIGHT } else { 0.0 };
        let insets = NSEdgeInsets { top, left: 0.0, bottom: 0.0, right: 0.0 };
        self.scroll.setAdditionalSafeAreaInsets(insets);
        if let Some(other) = self.other.borrow().as_ref() {
            other.setAdditionalSafeAreaInsets(insets);
        }
    }

    /// Puts `scroll` (a PDF's) in the column under the bar, kept clear of it as the note is.
    pub fn add_under(&self, scroll: &NSScrollView) {
        self.column.addSubview_positioned_relativeTo(scroll, objc2_app_kit::NSWindowOrderingMode::Below, Some(&self.bar));
        *self.other.borrow_mut() = Some(scroll.retain());
        self.inset();
    }

    /// Whether `view` is under the bar in the column (probe mode).
    pub fn is_over(&self, view: &NSView) -> bool {
        let subviews = self.column.subviews();
        let at = |v: &NSView| subviews.iter().position(|s| std::ptr::eq(&*s, v));
        matches!((at(&self.bar), at(view)), (Some(bar), Some(v)) if bar > v)
    }

    /// A search under way: the count says so.
    pub fn show_searching(&self) {
        self.count.setStringValue(&NSString::from_str(strings::FIND_SEARCHING));
    }

    /// Puts `text` in the field (probe mode: as if typed).
    pub fn set_text(&self, text: &str) {
        self.field.setStringValue(&NSString::from_str(text));
    }

    pub fn text(&self) -> String {
        self.field.stringValue().to_string()
    }

    /// `3 з 12`, `Немає` when nothing is found, empty without text to find.
    pub fn show_count(&self, current: Option<usize>, total: usize, searching: bool) {
        let text = match (current, searching) {
            (_, false) => String::new(),
            (Some(i), true) if total > 0 => strings::find_count(i + 1, total),
            _ => strings::FIND_NONE.to_owned(),
        };
        self.count.setStringValue(&NSString::from_str(&text));
    }

    /// `3 з 12`, as shown (probe mode).
    pub fn count_text(&self) -> String {
        self.count.stringValue().to_string()
    }
}

fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), Some(&NSString::from_str(description)))
}
