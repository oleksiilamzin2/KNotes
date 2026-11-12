//! A scroll view that keeps the toolbar's glass but drops its line (ADR 0016).
//!
//! On macOS 26 and later a scroll view under the toolbar gets scroll pockets: the glass that
//! fades content scrolling beneath the toolbar. They also draw a separator line along the
//! toolbar's bottom, which `titlebarSeparatorStyle` does not reach; a transparent titlebar
//! removes the line but the glass with it. The pockets have an `allowsSeparator` switch
//! (private): this view turns it off on its own pockets whenever they are added or laid out,
//! and only when the pocket answers the selector, so a future macOS can at worst bring the
//! line back.

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSResponder, NSScrollView, NSView};
use objc2_foundation::{NSObject, NSObjectProtocol};

define_class!(
    #[unsafe(super(NSScrollView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesQuietScrollView"]
    pub struct QuietScrollView;

    unsafe impl NSObjectProtocol for QuietScrollView {}

    impl QuietScrollView {
        #[unsafe(method(didAddSubview:))]
        fn did_add_subview(&self, subview: &NSView) {
            // SAFETY: forwarding the same call to the superclass.
            let _: () = unsafe { msg_send![super(self), didAddSubview: subview] };
            quiet(subview);
        }

        #[unsafe(method(layout))]
        fn layout(&self) {
            // SAFETY: forwarding the same call to the superclass.
            let _: () = unsafe { msg_send![super(self), layout] };
            for subview in self.subviews().iter() {
                quiet(&subview);
            }
        }
    }
);

impl QuietScrollView {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: plain NSView initialisation.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Whether `view` is a scroll pocket that can be told not to draw its separator.
pub fn is_pocket(view: &NSView) -> bool {
    AnyClass::get(c"NSScrollPocket").is_some_and(|class| view.isKindOfClass(class)) && view.respondsToSelector(sel!(setAllowsSeparator:))
}

fn quiet(view: &NSView) {
    if is_pocket(view) {
        // SAFETY: `is_pocket` checked that the view answers setAllowsSeparator:.
        let _: () = unsafe { msg_send![view, setAllowsSeparator: false] };
    }
}
