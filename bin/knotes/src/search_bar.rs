//! The list's search (ADR 0015), shown by the magnifying glass in the row under the toolbar:
//! the field takes the place of the list's name in that row (`toolbar::second_row` puts it
//! there), the mode switch a row of its own at the top of the list column, the list scrolling
//! beneath it. Beside the field, three segments left it a few letters wide.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use knotes_app::SearchMode;
use objc2_app_kit::{NSAutoresizingMaskOptions, NSControlSize, NSLayoutConstraint, NSScrollView, NSSearchField, NSSegmentDistribution, NSSegmentSwitchTracking, NSSegmentedControl, NSView};
use objc2_foundation::{NSArray, NSEdgeInsets, NSString};

use crate::strings;

/// Height of the switch's row: the switch and padding.
const BAR_HEIGHT: f64 = 32.0;
const SIDE_PADDING: f64 = 12.0;
/// Space above the switch.
const GAP: f64 = 6.0;

pub struct SearchBar {
    /// The list column: the switch's row over the note list's scroll view.
    pub column: Retained<NSView>,
    /// In the row under the toolbar, in place of the list's name while the search shows.
    pub field: Retained<NSSearchField>,
    /// Files, content or meaning: rewrites the query's operator (`content:`, `meaning:`), which
    /// stays the truth.
    pub mode: Retained<NSSegmentedControl>,
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
        // A field made in code does not scroll: typing past its width would hide the cursor.
        if let Some(cell) = field.cell() {
            cell.setScrollable(true);
            cell.setWraps(false);
            cell.setUsesSingleLineMode(true);
        }
        field.setHidden(true);
        let labels = NSArray::from_retained_slice(&[NSString::from_str(strings::SEARCH_FILES), NSString::from_str(strings::SEARCH_CONTENT), NSString::from_str(strings::SEARCH_MEANING)]);
        // SAFETY: labels and no target yet (`set_target` sets it).
        let mode = unsafe { NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(&labels, NSSegmentSwitchTracking::SelectOne, None, None, mtm) };
        mode.setSelectedSegment(0);
        mode.setControlSize(NSControlSize::Small);
        mode.setSegmentDistribution(NSSegmentDistribution::FillEqually);
        mode.setTranslatesAutoresizingMaskIntoConstraints(false);
        bar.addSubview(&mode);
        let safe = column.safeAreaLayoutGuide();
        let constraints = [
            bar.topAnchor().constraintEqualToAnchor(&safe.topAnchor()),
            bar.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            bar.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            bar.heightAnchor().constraintEqualToConstant(BAR_HEIGHT),
            mode.leadingAnchor().constraintEqualToAnchor_constant(&bar.leadingAnchor(), SIDE_PADDING),
            mode.trailingAnchor().constraintEqualToAnchor_constant(&bar.trailingAnchor(), -SIDE_PADDING),
            mode.topAnchor().constraintEqualToAnchor_constant(&bar.topAnchor(), GAP),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        Self { column, field, mode, bar, scroll }
    }

    /// `target` receives `searchChanged:` with the field as sender; it must outlive the window.
    pub fn set_target(&self, target: &AnyObject) {
        // SAFETY: the caller keeps the target alive as long as the field.
        unsafe {
            self.field.setTarget(Some(target));
            self.field.setAction(Some(sel!(searchChanged:)));
            self.mode.setTarget(Some(target));
            self.mode.setAction(Some(sel!(searchModeChanged:)));
        }
    }

    /// Shows the mode of the current query in the switch.
    pub fn show_mode(&self, mode: SearchMode) {
        self.mode.setSelectedSegment(match mode {
            SearchMode::Files => 0,
            // `block:` is a stricter content search.
            SearchMode::Content | SearchMode::Block => 1,
            SearchMode::Meaning => 2,
        });
    }

    /// Search by meaning on or off (its segment enabled), as the settings say (ADR 0029).
    pub fn set_meaning_on(&self, on: bool) {
        self.mode.setEnabled_forSegment(on, 2);
    }

    pub fn meaning_on(&self) -> bool {
        self.mode.isEnabledForSegment(2)
    }

    /// Puts `text` in the field with the cursor at its end.
    pub fn set_text(&self, text: &str) {
        self.field.setStringValue(&NSString::from_str(text));
        if let Some(window) = self.field.window() {
            window.makeFirstResponder(Some(&self.field));
        }
        if let Some(editor) = self.field.currentEditor() {
            let end = editor.string().length();
            editor.setSelectedRange(objc2_foundation::NSRange::new(end, 0));
        }
    }

    pub fn is_shown(&self) -> bool {
        !self.bar.isHidden()
    }

    /// Shows the field and the switch's row with the cursor in the field, or hides them and
    /// clears the field. The list gets the row's height as extra top inset, so no note hides
    /// behind it.
    pub fn show(&self, shown: bool) {
        self.bar.setHidden(!shown);
        self.field.setHidden(!shown);
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
