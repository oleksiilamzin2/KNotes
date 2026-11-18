//! The tabs along the toolbar (ADR 0024; the owner asked for them on top, as Safari's, where
//! they were rows of the sidebar): a strip that fills the toolbar between the sidebar's toggle
//! and the panel's, each tab a capsule as wide as the others, the one shown lighter. A click
//! shows a tab's document; the cross that shows with the pointer over a tab closes it; a tab's
//! menu closes it or every other. A tab dragged along the strip takes the place it is let go
//! at, the others making room as it passes.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBezierPath, NSColor, NSEvent, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSImage, NSImageSymbolConfiguration, NSMenu, NSMenuItem, NSResponder, NSStringDrawing,
    NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{NSArray, NSDictionary, NSObject, NSPoint, NSRect, NSSize, NSString, NSZeroRect, ns_string};

use crate::strings;

/// The strip's height, and the gap between two tabs.
pub const HEIGHT: f64 = 36.0;
const GAP: f64 = 4.0;
/// A tab shows its name while it is at least this wide.
const NAMED: f64 = 56.0;
/// How far a tab's piece of glass is from the strip's edge.
const INSET: f64 = 2.0;
/// The cross's room at a tab's left, and the symbol's size.
const CROSS: f64 = 26.0;
const SYMBOL: f64 = 14.0;

/// A tab as the strip shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    /// What it is a tab of: a document's vault path, or a deck's (`deck://…`).
    pub path: String,
    pub name: String,
    /// Its SF Symbol: a note's, a PDF's, a deck's.
    pub symbol: &'static str,
    /// The space its document is in, when that is not the window's (ADR 0030): its colour,
    /// shown as a dot after the name, and its name.
    pub space: Option<(knotes_spaces::SpaceColor, String)>,
    /// Its document is kept from the AI (ADR 0036): marked after the name.
    pub kept_from_ai: bool,
}

type OnTab = Box<dyn Fn(String)>;

#[derive(Default)]
pub struct StripIvars {
    tabs: RefCell<Vec<Tab>>,
    /// The tab shown.
    current: Cell<Option<usize>>,
    /// The tab under the pointer, and whether the pointer is over its cross.
    hover: Cell<Option<(usize, bool)>>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
    on_select: RefCell<Option<OnTab>>,
    on_close: RefCell<Option<OnTab>>,
    on_close_others: RefCell<Option<OnTab>>,
    /// Called with a tab's path and the place it was dragged to.
    on_move: RefCell<Option<Box<dyn Fn(String, usize)>>>,
    /// The tab pressed, and how far into it (x): it is dragged once the pointer moves on.
    pressed: Cell<Option<(usize, f64)>>,
    /// The tab being dragged: which, where its left edge is now, and the place it would take.
    dragged: Cell<Option<(usize, f64, usize)>>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesTabStrip"]
    #[ivars = StripIvars]
    pub struct TabStrip;

    impl TabStrip {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// A click on a tab is the tab's, not a drag of the window by its toolbar.
        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            if let Some(old) = self.ivars().tracking.borrow_mut().take() {
                self.removeTrackingArea(&old);
            }
            let options = NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::ActiveInActiveApp | NSTrackingAreaOptions::InVisibleRect;
            // SAFETY: the view owns the area and removes it before making another.
            let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(<NSTrackingArea as objc2::AnyThread>::alloc(), NSZeroRect, options, Some(self), None) };
            self.addTrackingArea(&area);
            *self.ivars().tracking.borrow_mut() = Some(area);
            // SAFETY: the superclass's own handling.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.point_at(self.tab_at(self.convertPoint_fromView(event.locationInWindow(), None)));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.point_at(None);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            // A press on a tab's body may begin a drag; one on its cross closes it at once.
            let body = self.tab_at(at).filter(|(_, cross)| !cross).map(|(tab, _)| tab);
            self.press(at);
            let pressed = body.and_then(|tab| self.places().get(tab).map(|place| (tab, at.x - place.origin.x)));
            self.ivars().pressed.set(pressed);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            self.drag_to(at.x);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.let_go();
        }

        /// A tab's menu: it closes, or every other does.
        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
            self.tab_menu(event)
        }

        #[unsafe(method(closeTab:))]
        fn close_tab(&self, item: &NSMenuItem) {
            self.close(item.tag() as usize);
        }

        #[unsafe(method(closeOtherTabs:))]
        fn close_other_tabs(&self, item: &NSMenuItem) {
            self.close_others(item.tag() as usize);
        }
    }
);

/// Where a tab at `place` shows its cross: at its left; in its middle when it has no room for
/// a name. Drawn there and clicked there.
fn cross_place(place: NSRect) -> NSRect {
    let middle = place.origin.y + place.size.height / 2.0;
    const SIZE: f64 = 15.0;
    // Clear of the round end of the strip's first tab.
    let left = if place.size.width < NAMED { place.origin.x + (place.size.width - SIZE) / 2.0 } else { place.origin.x + 9.0 };
    rect(left, middle - SIZE / 2.0, SIZE, SIZE)
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

fn attributes(font: &NSFont, colour: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &[font.as_ref() as &AnyObject, colour.as_ref()])
}

/// `name` cut with «…» to fit `room` points in `font`.
fn fitted(name: &str, font: &NSFont, room: f64) -> (String, f64) {
    let width = |text: &str| unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes(font, &NSColor::labelColor()))) }.width;
    let whole = width(name);
    if whole <= room {
        return (name.to_owned(), whole);
    }
    let letters: Vec<char> = name.chars().collect();
    // The longest start that fits with its «…».
    let (mut low, mut high) = (0, letters.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let cut: String = letters[..middle].iter().chain(['…'].iter()).collect();
        if width(&cut) <= room { low = middle } else { high = middle - 1 }
    }
    let cut: String = letters[..low].iter().collect::<String>().trim_end().to_owned() + "…";
    let fits = width(&cut);
    (cut, fits)
}

/// The cross: a filled circle of `circle` with its cross cut in `cross` (the symbol's two
/// layers; in one colour the cross did not show on its circle).
fn cross_symbol(cross: &NSColor, circle: &NSColor) -> Option<Retained<NSImage>> {
    let tint = NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&[cross.copy_colour(), circle.copy_colour()]));
    NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("xmark.circle.fill"), None).and_then(|image| image.imageWithSymbolConfiguration(&tint))
}

fn symbol(name: &str, colour: &NSColor) -> Option<Retained<NSImage>> {
    let tint = NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&[colour.copy_colour()]));
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None).and_then(|image| image.imageWithSymbolConfiguration(&tint))
}

trait CopyColour {
    fn copy_colour(&self) -> Retained<NSColor>;
}

impl CopyColour for NSColor {
    fn copy_colour(&self) -> Retained<NSColor> {
        self.colorWithAlphaComponent(self.alphaComponent())
    }
}

/// `image` drawn in the middle of `into`, its proportions kept.
fn draw_symbol(image: &NSImage, into: NSRect) {
    let size = image.size();
    let scale = (into.size.width / size.width).min(into.size.height / size.height);
    let (width, height) = (size.width * scale, size.height * scale);
    image.drawInRect(rect(into.origin.x + (into.size.width - width) / 2.0, into.origin.y + (into.size.height - height) / 2.0, width, height));
}

impl TabStrip {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(StripIvars::default())), init] }
    }

    /// The menu of the tab under the event: it closes, or every other does.
    fn tab_menu(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
        let (tab, _) = self.tab_at(self.convertPoint_fromView(event.locationInWindow(), None))?;
        let menu = NSMenu::new(self.mtm());
        for (title, action) in [(strings::CLOSE_TAB, sel!(closeTab:)), (strings::CLOSE_OTHER_TABS, sel!(closeOtherTabs:))] {
            // SAFETY: the strip outlives its menu and takes the action.
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm()), &NSString::from_str(title), Some(action), ns_string!("")) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            item.setTag(tab as isize);
            menu.addItem(&item);
        }
        Some(menu)
    }

    /// Called with the tab clicked, the tab closed, and the tab all others close for.
    pub fn on_select(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_select.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_close(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_close.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_close_others(&self, callback: impl Fn(String) + 'static) {
        *self.ivars().on_close_others.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `tabs`, `current` the one whose document shows.
    pub fn set(&self, tabs: Vec<Tab>, current: Option<usize>) {
        if *self.ivars().tabs.borrow() == tabs && self.ivars().current.get() == current {
            return;
        }
        *self.ivars().tabs.borrow_mut() = tabs;
        self.ivars().current.set(current);
        self.ivars().hover.set(None);
        self.setNeedsDisplay(true);
    }

    /// Called with a tab's path and the place among the tabs it was dragged to.
    pub fn on_move(&self, callback: impl Fn(String, usize) + 'static) {
        *self.ivars().on_move.borrow_mut() = Some(Box::new(callback));
    }

    /// The pointer at `x` with a tab pressed: the tab follows it (after a few points: a click
    /// that slips is no drag), and takes the place its middle is over.
    fn drag_to(&self, x: f64) {
        let Some((tab, grab)) = self.ivars().pressed.get() else { return };
        let places = self.places();
        let Some(place) = places.get(tab) else { return };
        let left = (x - grab).clamp(0.0, (self.bounds().size.width - place.size.width).max(0.0));
        if self.ivars().dragged.get().is_none() && (left - place.origin.x).abs() < 5.0 {
            return;
        }
        let slot = place.size.width + GAP;
        let over = (((left + place.size.width / 2.0) / slot) as usize).min(places.len() - 1);
        self.ivars().dragged.set(Some((tab, left, over)));
        self.ivars().hover.set(None);
        self.setNeedsDisplay(true);
    }

    /// The button let go: a tab dragged takes its place.
    fn let_go(&self) {
        self.ivars().pressed.set(None);
        if let Some((tab, _, to)) = self.ivars().dragged.take() {
            self.setNeedsDisplay(true);
            self.move_tab(tab, to);
        }
    }

    /// Tab `tab` moved to place `to` (also the probe's drag).
    pub fn move_tab(&self, tab: usize, to: usize) {
        let path = self.ivars().tabs.borrow().get(tab).map(|t| t.path.clone());
        if let (Some(path), Some(callback), true) = (path, self.ivars().on_move.borrow().as_ref(), tab != to) {
            callback(path, to);
        }
    }

    /// The tab clicked (also the probe's click).
    pub fn click(&self, tab: usize) {
        self.tell(&self.ivars().on_select, tab);
    }

    pub fn close(&self, tab: usize) {
        self.tell(&self.ivars().on_close, tab);
    }

    pub fn close_others(&self, tab: usize) {
        self.tell(&self.ivars().on_close_others, tab);
    }

    fn tell(&self, callback: &RefCell<Option<OnTab>>, tab: usize) {
        // Not borrowed while told: what is told sets the tabs again.
        let path = self.ivars().tabs.borrow().get(tab).map(|t| t.path.clone());
        if let (Some(path), Some(callback)) = (path, callback.borrow().as_ref()) {
            callback(path);
        }
    }

    /// The tab of `path`, if it has one.
    pub fn tab_of(&self, path: &str) -> Option<usize> {
        self.ivars().tabs.borrow().iter().position(|tab| tab.path == path)
    }

    /// The pointer over `tab` (none: gone), as the tracking area tells it (probe mode).
    pub fn hover(&self, tab: Option<usize>) {
        self.point_at(tab.map(|tab| (tab, false)));
    }

    /// Whether each tab shows its cross: `shown hidden` (probe mode).
    pub fn crosses(&self) -> String {
        let hover = self.ivars().hover.get();
        (0..self.ivars().tabs.borrow().len()).map(|i| if hover.is_some_and(|(at, _)| at == i) { "shown" } else { "hidden" }).collect::<Vec<_>>().join(" ")
    }

    /// The name of the tab shown (probe mode).
    pub fn shown(&self) -> Option<String> {
        self.ivars().current.get().and_then(|i| self.ivars().tabs.borrow().get(i).map(|tab| tab.name.clone()))
    }

    /// The tabs marked, each with its marks: `c • Дім | a • no-ai` (probe mode).
    pub fn marks(&self) -> String {
        let marked: Vec<String> = self
            .ivars()
            .tabs
            .borrow()
            .iter()
            .filter(|tab| tab.space.is_some() || tab.kept_from_ai)
            .map(|tab| {
                let space = tab.space.iter().map(|(_, name)| name.as_str());
                let marks: Vec<&str> = space.chain(tab.kept_from_ai.then_some("no-ai")).collect();
                format!("{} • {}", tab.name, marks.join(", "))
            })
            .collect();
        marked.join(" | ")
    }

    /// The tabs' names (probe mode).
    pub fn names(&self) -> Vec<String> {
        self.ivars().tabs.borrow().iter().map(|tab| tab.name.clone()).collect()
    }

    /// The titles of a tab's menu (probe mode).
    pub fn menu_titles(&self) -> Vec<String> {
        vec![strings::CLOSE_TAB.to_owned(), strings::CLOSE_OTHER_TABS.to_owned()]
    }

    fn point_at(&self, tab: Option<(usize, bool)>) {
        if self.ivars().hover.replace(tab) != tab {
            self.setNeedsDisplay(true);
        }
    }

    /// Each tab's place: all as wide, together filling the strip (as Safari's do).
    fn places(&self) -> Vec<NSRect> {
        let count = self.ivars().tabs.borrow().len();
        if count == 0 {
            return Vec::new();
        }
        let bounds = self.bounds();
        let width = ((bounds.size.width - GAP * (count - 1) as f64) / count as f64).max(1.0);
        (0..count).map(|i| rect(i as f64 * (width + GAP), 0.0, width, bounds.size.height)).collect()
    }

    /// The tab at `at`, and whether `at` is on its cross (or within a few points of it).
    fn tab_at(&self, at: NSPoint) -> Option<(usize, bool)> {
        let places = self.places();
        let tab = places.iter().position(|place| at.x >= place.origin.x && at.x < place.origin.x + place.size.width && at.y >= 0.0 && at.y <= place.size.height)?;
        let cross = cross_place(places[tab]);
        let near = 5.0;
        let on_cross = at.x >= cross.origin.x - near && at.x <= cross.origin.x + cross.size.width + near && at.y >= cross.origin.y - near && at.y <= cross.origin.y + cross.size.height + near;
        Some((tab, on_cross))
    }

    /// A press at `at`: on a tab's cross it closes the tab, elsewhere on a tab it shows it.
    pub fn press(&self, at: NSPoint) {
        match self.tab_at(at) {
            Some((tab, true)) => self.close(tab),
            Some((tab, false)) => self.click(tab),
            None => {}
        }
    }

    /// The middle of `tab`'s cross (probe mode: where a click on it lands).
    pub fn cross_middle(&self, tab: usize) -> Option<NSPoint> {
        let cross = cross_place(*self.places().get(tab)?);
        Some(NSPoint::new(cross.origin.x + cross.size.width / 2.0, cross.origin.y + cross.size.height / 2.0))
    }

    fn draw(&self) {
        let tabs = self.ivars().tabs.borrow();
        let (current, hover) = (self.ivars().current.get(), self.ivars().hover.get());
        let font = NSFont::systemFontOfSize(13.0);
        // The strip lies on the toolbar's own glass: the tab shown is a lighter piece of it, the
        // others the glass itself, a little darker under the pointer, a hairline between two.
        let dark = NSColor::textBackgroundColor().colorUsingColorSpace(&objc2_app_kit::NSColorSpace::sRGBColorSpace()).is_some_and(|c| c.redComponent() < 0.5);
        let mut places = self.places();
        // While a tab is dragged the others stand as if it were at the place it would take,
        // and it is drawn last, where the pointer has it.
        let dragged = self.ivars().dragged.get().filter(|(tab, _, _)| *tab < places.len());
        if let Some((tab, left, over)) = dragged {
            let slots = places.clone();
            let mut others: Vec<usize> = (0..slots.len()).filter(|i| *i != tab).collect();
            others.insert(over.min(others.len()), tab);
            for (slot, index) in others.into_iter().enumerate() {
                places[index] = slots[slot];
            }
            places[tab].origin.x = left;
        }
        // The strip itself: a grey capsule the tabs lie in (the owner's reference), whatever
        // the toolbar draws under it.
        let bounds = self.bounds();
        NSColor::labelColor().colorWithAlphaComponent(if dark { 0.16 } else { 0.09 }).setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, bounds.size.height / 2.0, bounds.size.height / 2.0).fill();
        let last = dragged.map(|(tab, _, _)| tab);
        let order = (0..tabs.len()).filter(|i| Some(*i) != last).chain(last);
        for (i, tab, place) in order.filter_map(|i| Some((i, tabs.get(i)?, *places.get(i)?))) {
            let shown = current == Some(i);
            let pointed = hover.is_some_and(|(at, _)| at == i);
            let piece = rect(place.origin.x + INSET, place.origin.y + INSET, place.size.width - 2.0 * INSET, place.size.height - 2.0 * INSET);
            let capsule = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(piece, piece.size.height / 2.0, piece.size.height / 2.0);
            if shown {
                // White on the grey, as the reference's; in the dark a lighter grey.
                NSColor::whiteColor().colorWithAlphaComponent(if dark { 0.22 } else { 1.0 }).setFill();
                capsule.fill();
            } else if pointed || last == Some(i) {
                NSColor::labelColor().colorWithAlphaComponent(0.10).setFill();
                capsule.fill();
            }
            // Between two tabs neither of which is shown or pointed at.
            let quiet = |at: usize| current != Some(at) && !hover.is_some_and(|(pointed, _)| pointed == at);
            if dragged.is_none() && i > 0 && quiet(i) && quiet(i - 1) {
                NSColor::separatorColor().setFill();
                NSBezierPath::fillRect(rect(place.origin.x - GAP / 2.0 - 0.5, place.origin.y + 10.0, 1.0, place.size.height - 20.0));
            }
            // Every tab's name dark, as Safari's (the owner's reference): the tab shown is told
            // by its white piece; the others' names are a shade quieter, no more.
            let ink = if shown { NSColor::labelColor() } else { NSColor::labelColor().colorWithAlphaComponent(0.85) };
            let middle = place.origin.y + place.size.height / 2.0;
            // With the pointer over the tab, its cross at its left: a filled circle, as Safari's.
            let over_cross = hover.is_some_and(|(_, cross)| cross);
            // A dark circle with a light cross, darker with the pointer on it, as Safari's.
            let circle = NSColor::labelColor().colorWithAlphaComponent(if over_cross { 0.75 } else { 0.5 });
            let cross = if pointed { cross_symbol(&NSColor::textBackgroundColor(), &circle) } else { None };
            let mark = symbol(tab.symbol, &ink);
            if place.size.width < NAMED {
                // No room for a name: the cross in the symbol's place.
                match cross {
                    Some(cross) => draw_symbol(&cross, cross_place(place)),
                    None => {
                        if let Some(mark) = mark {
                            draw_symbol(&mark, rect(place.origin.x + (place.size.width - SYMBOL) / 2.0, middle - SYMBOL / 2.0, SYMBOL, SYMBOL));
                        }
                    }
                }
                continue;
            }
            if let Some(cross) = cross {
                draw_symbol(&cross, cross_place(place));
            }
            // The symbol, the name and its marks together in the tab's middle, clear of the
            // cross's room.
            let marks = crate::row_marks::width(tab.space.is_some(), tab.kept_from_ai);
            let room = place.size.width - 2.0 * CROSS - SYMBOL - 5.0 - marks;
            let (name, width) = fitted(&tab.name, &font, room.max(1.0));
            let left = place.origin.x + (place.size.width - (SYMBOL + 5.0 + width + marks)) / 2.0;
            // Another space's dot, and the AI's mark, as the list's rows have them.
            crate::row_marks::draw(tab.space.as_ref().map(|(colour, _)| *colour), tab.kept_from_ai, left + SYMBOL + 5.0 + width, middle, &NSColor::secondaryLabelColor());
            if let Some(mark) = mark {
                draw_symbol(&mark, rect(left, middle - SYMBOL / 2.0, SYMBOL, SYMBOL));
            }
            // SAFETY: a font and a colour are those attributes' values.
            unsafe { NSString::from_str(&name).drawAtPoint_withAttributes(NSPoint::new(left + SYMBOL + 5.0, middle - (font.ascender() - font.descender()) / 2.0), Some(&attributes(&font, &ink))) };
        }
    }
}
