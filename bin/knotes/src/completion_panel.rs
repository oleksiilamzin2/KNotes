//! The floating list of link completions under the cursor (ADR 0013). It never takes the
//! focus: the text view keeps typing and forwards Up/Down/Return/Tab/Esc to it.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSFont, NSLineBreakMode, NSPanel, NSTextAlignment, NSTextField, NSView,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

const WIDTH: f64 = 360.0;
const ROW_HEIGHT: f64 = 26.0;
const PADDING: f64 = 4.0;
/// Text inset inside a row.
const ROW_INSET: f64 = 8.0;
pub const MAX_ROWS: usize = 8;

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub title: String,
    /// Folder of a note, empty for the vault root and for headings.
    pub detail: String,
    /// Text that replaces what was typed after `[[` (or after `#`).
    pub insert: String,
}

pub struct CompletionPanel {
    panel: Retained<NSPanel>,
    /// Holds one view per row, laid out by hand (top row first).
    list: Retained<NSView>,
    items: RefCell<Vec<CompletionItem>>,
    selected: Cell<usize>,
}

impl CompletionPanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, ROW_HEIGHT));
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(NSPanel::alloc(mtm), frame, style, NSBackingStoreType::Buffered, true);
        panel.setHasShadow(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        // SAFETY: the panel is owned by this struct, not released on close.
        unsafe { panel.setReleasedWhenClosed(false) };
        let background = NSVisualEffectView::new(mtm);
        background.setMaterial(NSVisualEffectMaterial::Menu);
        background.setState(NSVisualEffectState::Active);
        background.setWantsLayer(true);
        if let Some(layer) = background.layer() {
            layer.setCornerRadius(8.0);
            layer.setMasksToBounds(true);
        }
        let list = NSView::new(mtm);
        list.setFrame(frame);
        list.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        background.addSubview(&list);
        panel.setContentView(Some(&background));
        Self { panel, list, items: RefCell::new(Vec::new()), selected: Cell::new(0) }
    }

    /// Shows `items` (first one chosen) with the top-left corner just under `cursor`, a rect
    /// in screen coordinates.
    pub fn show(&self, parent: &NSWindow, cursor: NSRect, items: Vec<CompletionItem>) {
        let height = items.len() as f64 * ROW_HEIGHT + 2.0 * PADDING;
        *self.items.borrow_mut() = items;
        self.selected.set(0);
        self.rebuild();
        let origin = NSPoint::new(cursor.origin.x - PADDING, cursor.origin.y - height - 2.0);
        self.panel.setFrame_display(NSRect::new(origin, NSSize::new(WIDTH, height)), true);
        if self.panel.parentWindow().is_none() {
            // SAFETY: both windows are alive; the child follows the parent's moves.
            unsafe { parent.addChildWindow_ordered(&self.panel, NSWindowOrderingMode::Above) };
        }
    }

    pub fn hide(&self) {
        if !self.is_visible() {
            return;
        }
        if let Some(parent) = self.panel.parentWindow() {
            parent.removeChildWindow(&self.panel);
        }
        self.panel.orderOut(None);
        self.items.borrow_mut().clear();
    }

    pub fn is_visible(&self) -> bool {
        !self.items.borrow().is_empty()
    }

    /// Up (`-1`) or down (`1`), wrapping around.
    pub fn move_selection(&self, delta: isize) {
        let n = self.items.borrow().len() as isize;
        if n > 0 {
            self.selected.set((self.selected.get() as isize + delta).rem_euclid(n) as usize);
            self.rebuild();
        }
    }

    pub fn chosen(&self) -> Option<CompletionItem> {
        self.items.borrow().get(self.selected.get()).cloned()
    }

    /// `▸Title — Folder | Title…` or `none`, for probe mode.
    pub fn describe(&self) -> String {
        let items = self.items.borrow();
        if items.is_empty() {
            return "none".into();
        }
        let rows: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let mark = if i == self.selected.get() { "▸" } else { "" };
                if item.detail.is_empty() { format!("{mark}{}", item.title) } else { format!("{mark}{} — {}", item.title, item.detail) }
            })
            .collect();
        rows.join(" | ")
    }

    fn rebuild(&self) {
        let mtm = self.panel.mtm();
        for old in self.list.subviews().iter() {
            old.removeFromSuperview();
        }
        let items = self.items.borrow();
        let n = items.len();
        for (i, item) in items.iter().enumerate() {
            // Not flipped: the first row is the highest.
            let y = PADDING + (n - 1 - i) as f64 * ROW_HEIGHT;
            let frame = NSRect::new(NSPoint::new(PADDING, y), NSSize::new(WIDTH - 2.0 * PADDING, ROW_HEIGHT));
            self.list.addSubview(&row(mtm, item, i == self.selected.get(), frame));
        }
    }
}

/// One row: the title on the left (truncated), the folder or heading level on the right.
fn row(mtm: MainThreadMarker, item: &CompletionItem, selected: bool, frame: NSRect) -> Retained<NSView> {
    let view = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let label = |text: &str, size: f64, color: Retained<NSColor>| {
        let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        field.setFont(Some(&NSFont::systemFontOfSize(size)));
        field.setTextColor(Some(&color));
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        field
    };
    let (main, secondary) = if selected {
        (NSColor::alternateSelectedControlTextColor(), NSColor::alternateSelectedControlTextColor())
    } else {
        (NSColor::labelColor(), NSColor::secondaryLabelColor())
    };
    let title = label(&item.title, 13.0, main);
    let detail = label(&item.detail, 11.0, secondary);
    detail.setAlignment(NSTextAlignment::Right);
    let inner = frame.size.width - 2.0 * ROW_INSET;
    let detail_width = if item.detail.is_empty() { 0.0 } else { detail.fittingSize().width.min(inner * 0.45) };
    let title_height = title.fittingSize().height;
    let detail_height = detail.fittingSize().height;
    title.setFrame(NSRect::new(
        NSPoint::new(ROW_INSET, (ROW_HEIGHT - title_height) / 2.0),
        NSSize::new(inner - detail_width - if detail_width > 0.0 { 8.0 } else { 0.0 }, title_height),
    ));
    detail.setFrame(NSRect::new(
        NSPoint::new(ROW_INSET + inner - detail_width, (ROW_HEIGHT - detail_height) / 2.0),
        NSSize::new(detail_width, detail_height),
    ));
    view.addSubview(&title);
    view.addSubview(&detail);
    if selected {
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setBackgroundColor(Some(&NSColor::selectedContentBackgroundColor().CGColor()));
            layer.setCornerRadius(5.0);
        }
    }
    view
}
