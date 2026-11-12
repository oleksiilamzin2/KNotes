//! Watches a split view item's `collapsed` property (KVO): the sidebar button, the divider
//! and the View menu all collapse the sidebar through it.

use std::cell::RefCell;
use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSSplitViewItem;
use objc2_foundation::{NSDictionary, NSKeyValueChangeKey, NSKeyValueObservingOptions, NSObject, NSObjectNSKeyValueObserverRegistration, NSObjectProtocol, NSString, ns_string};

type OnChange = Box<dyn Fn(bool)>;

#[derive(Default)]
pub struct Ivars {
    on_change: RefCell<Option<OnChange>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesCollapseObserver"]
    #[ivars = Ivars]
    pub struct CollapseObserver;

    unsafe impl NSObjectProtocol for CollapseObserver {}

    impl CollapseObserver {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe(
            &self,
            _key_path: Option<&NSString>,
            object: Option<&AnyObject>,
            _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            let collapsed = object.and_then(|o| o.downcast_ref::<NSSplitViewItem>()).is_some_and(|item| item.isCollapsed());
            if let Some(callback) = self.ivars().on_change.borrow().as_ref() {
                callback(collapsed);
            }
        }
    }
);

impl CollapseObserver {
    /// Calls `on_change(collapsed)` now and on every change of `item.collapsed`. Call `stop`
    /// before the observer goes away.
    pub fn watch(mtm: MainThreadMarker, item: &NSSplitViewItem, on_change: impl Fn(bool) + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { on_change: RefCell::new(Some(Box::new(on_change))) });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let options = NSKeyValueObservingOptions::Initial | NSKeyValueObservingOptions::New;
        // SAFETY: the observer outlives the registration (`stop` removes it); no context.
        unsafe { item.addObserver_forKeyPath_options_context(&this, ns_string!("collapsed"), options, std::ptr::null_mut()) };
        this
    }

    pub fn stop(&self, item: &NSSplitViewItem) {
        // SAFETY: removes the registration made by `watch`.
        unsafe { item.removeObserver_forKeyPath(self, ns_string!("collapsed")) };
    }
}
