//! Quick Look for an image in a note (ADR 0020): the system's preview panel, with its zoom and
//! full screen. The note's text view controls the panel and hands it this source.

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSInteger, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource};

define_class!(
    /// One file for the preview panel.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesQuickLookSource"]
    #[ivars = Retained<NSURL>]
    pub struct QuickLookSource;

    unsafe impl NSObjectProtocol for QuickLookSource {}

    unsafe impl QLPreviewPanelDataSource for QuickLookSource {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            1
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(&self, _panel: Option<&QLPreviewPanel>, _index: NSInteger) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            Some(ProtocolObject::from_retained(self.ivars().clone()))
        }
    }
);

impl QuickLookSource {
    pub fn new(mtm: MainThreadMarker, file: &Path) -> Retained<Self> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
        let this = Self::alloc(mtm).set_ivars(url);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}
