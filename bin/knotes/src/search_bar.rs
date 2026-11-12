//! The search row at the top of the note list column (ADR 0015): shown by the magnifying
//! glass in the toolbar, it spans the column under the toolbar while the list scrolls
//! beneath it. A field in the toolbar itself would not fit next to the list title.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSLayoutConstraint, NSScrollView, NSSearchField, NSView};
use objc2_foundation::{NSArray, NSEdgeInsets, NSString};

use crate::strings;

/// Height of the search row, field and padding.
const BAR_HEIGHT: f64 = 40.0;
const SIDE_PADDING: f64 = 12.0;

pub struct SearchBar {
    /// The list column: the search row over the note list's scroll view.
    pub column: Retained<NSView>,
    pub field: Retained<NSSearchField>,
    bar: Retained<NSView>,
    scroll: Retained<NSScrollView>,
}

impl SearchBar {
    pub fn new(mtm: MainThreadMarker, scroll: Retained<NSScrollView>) -> Self {
        let column = NSView::new(mtm);
        scroll.setFrame(column.bounds());
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        column.addSubview(&scroll);
        let bar = NSView::new(mtm);
        bar.setTranslatesAutoresizingMaskIntoConstraints(false);
        bar.setHidden(true);
        column.addSubview(&bar);
        let field = NSSearchField::new(mtm);
        field.setPlaceholderString(Some(&NSString::from_str(strings::SEARCH)));
        field.setTranslatesAutoresizingMaskIntoConstraints(false);
        bar.addSubview(&field);
        let safe = column.safeAreaLayoutGuide();
        let constraints = [
            bar.topAnchor().constraintEqualToAnchor(&safe.topAnchor()),
            bar.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            bar.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            bar.heightAnchor().constraintEqualToConstant(BAR_HEIGHT),
            field.leadingAnchor().constraintEqualToAnchor_constant(&bar.leadingAnchor(), SIDE_PADDING),
            field.trailingAnchor().constraintEqualToAnchor_constant(&bar.trailingAnchor(), -SIDE_PADDING),
            field.centerYAnchor().constraintEqualToAnchor(&bar.centerYAnchor()),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        Self { column, field, bar, scroll }
    }

    /// `target` receives `searchChanged:` with the field as sender; it must outlive the window.
    pub fn set_target(&self, target: &AnyObject) {
        // SAFETY: the caller keeps the target alive as long as the field.
        unsafe {
            self.field.setTarget(Some(target));
            self.field.setAction(Some(sel!(searchChanged:)));
        }
    }

    pub fn is_shown(&self) -> bool {
        !self.bar.isHidden()
    }

    /// Shows the row with the cursor in the field, or hides it and clears the field. The list
    /// gets the row's height as extra top inset, so no note hides behind it.
    pub fn show(&self, shown: bool) {
        self.bar.setHidden(!shown);
        let top = if shown { BAR_HEIGHT } else { 0.0 };
        self.scroll.setAdditionalSafeAreaInsets(NSEdgeInsets { top, left: 0.0, bottom: 0.0, right: 0.0 });
        if shown {
            if let Some(window) = self.field.window() {
                window.makeFirstResponder(Some(&self.field));
            }
        } else {
            self.field.setStringValue(&NSString::new());
        }
    }
}
