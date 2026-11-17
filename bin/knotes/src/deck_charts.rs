//! A deck's statistics as charts (ADR 0037; the owner asked for charts as Anki's): what its
//! cards are (new, young, mature), what comes due each of the next days, the last days'
//! reviews (recalled and forgotten), and their grades. One view that draws them all, under the
//! line saying how many cards wait; the bar under the pointer says its day and its number.

use std::cell::{Cell, RefCell};

use knotes_core::DeckStats;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSBezierPath, NSColor, NSColorSpace, NSEvent, NSFont, NSFontAttributeName, NSFontWeightSemibold, NSForegroundColorAttributeName, NSGraphicsContext, NSResponder, NSStringDrawing, NSTrackingArea,
    NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{NSDictionary, NSObject, NSPoint, NSRect, NSSize, NSString, NSZeroRect};

use crate::strings;

/// The height the charts take, whatever their width.
pub const HEIGHT: f64 = 676.0;

const HEADING: f64 = 44.0;
const GAP: f64 = 28.0;
/// Where each panel starts, from the top.
const CARDS_TOP: f64 = HEADING;
const DUE_TOP: f64 = CARDS_TOP + 72.0 + GAP;
const REVIEWS_TOP: f64 = DUE_TOP + 160.0 + GAP;
const GRADES_TOP: f64 = REVIEWS_TOP + 182.0 + GAP;
/// A plot: under its panel's title, this high, its numbers left of it.
const PLOT_TOP: f64 = 30.0;
const PLOT_HEIGHT: f64 = 110.0;
const PLOT_LEFT: f64 = 30.0;
/// A grade's row, and where its bar starts.
const GRADE_ROW: f64 = 22.0;
const GRADE_LEFT: f64 = 64.0;

/// The plot the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plot {
    Due,
    Reviews,
}

#[derive(Default)]
pub struct ChartsIvars {
    stats: RefCell<Option<DeckStats>>,
    /// The line above the charts: how many cards wait.
    heading: RefCell<String>,
    /// The bar under the pointer.
    hover: Cell<Option<(Plot, usize)>>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesDeckCharts"]
    #[ivars = ChartsIvars]
    pub struct DeckCharts;

    impl DeckCharts {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
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
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            self.point_at(self.bar_at(at));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.point_at(None);
        }
    }
);

/// A chart's colours, for the light look or the dark: the cards' three kinds (the recalled and
/// the forgotten take the first two). The palette's first three hues: apart for every eye.
fn series(dark: bool) -> [Retained<NSColor>; 3] {
    let hues: [u32; 3] = if dark { [0x3987e5, 0xd95926, 0x199e70] } else { [0x2a78d6, 0xeb6834, 0x1baf7a] };
    hues.map(|hex| NSColor::colorWithSRGBRed_green_blue_alpha(f64::from((hex >> 16) & 0xff) / 255.0, f64::from((hex >> 8) & 0xff) / 255.0, f64::from(hex & 0xff) / 255.0, 1.0))
}

/// Whether the view draws in the dark look now (colours resolve by the look drawn in).
fn dark() -> bool {
    NSColor::textBackgroundColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()).is_some_and(|c| c.redComponent() < 0.5)
}

fn attributes(font: &NSFont, colour: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &[font.as_ref() as &AnyObject, colour.as_ref()])
}

fn text_width(text: &str, font: &NSFont) -> f64 {
    // SAFETY: a font and a colour are those attributes' values.
    unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes(font, &NSColor::labelColor()))) }.width
}

/// How `text` stands to the x it is drawn at.
#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Middle,
    Right,
}

fn draw_text(text: &str, x: f64, y: f64, font: &NSFont, colour: &NSColor, align: Align) {
    let x = match align {
        Align::Left => x,
        Align::Middle => x - text_width(text, font) / 2.0,
        Align::Right => x - text_width(text, font),
    };
    // SAFETY: as above.
    unsafe { NSString::from_str(text).drawAtPoint_withAttributes(NSPoint::new(x, y), Some(&attributes(font, colour))) };
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

/// A bar standing on its base (`rect`'s bottom), its top's corners round when `round`.
fn fill_bar(bar: NSRect, round: bool) {
    let radius = (bar.size.width / 2.0).min(3.0).min(bar.size.height);
    if !round || radius < 1.0 {
        NSBezierPath::fillRect(bar);
        return;
    }
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bar, radius, radius).fill();
    if bar.size.height > radius {
        NSBezierPath::fillRect(rect(bar.origin.x, bar.origin.y + radius, bar.size.width, bar.size.height - radius));
    }
}

/// The top of a plot's scale for its largest value: 1, 2 or 5 times a power of ten.
fn scale_top(most: usize) -> usize {
    let mut power = 1;
    loop {
        for times in [1, 2, 5] {
            if times * power >= most.max(1) {
                return times * power;
            }
        }
        power *= 10;
    }
}

impl DeckCharts {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(ChartsIvars::default())), init] }
    }

    /// Shows `stats` under `heading`; `said`: the same in words, for VoiceOver.
    pub fn set(&self, heading: &str, stats: DeckStats, said: &str) {
        *self.ivars().heading.borrow_mut() = heading.to_owned();
        *self.ivars().stats.borrow_mut() = Some(stats);
        self.ivars().hover.set(None);
        self.setAccessibilityLabel(Some(&NSString::from_str(&format!("{heading}\n{said}"))));
        self.setNeedsDisplay(true);
    }

    fn point_at(&self, bar: Option<(Plot, usize)>) {
        if self.ivars().hover.replace(bar) != bar {
            self.setNeedsDisplay(true);
        }
    }

    /// A plot's place: its bars' area.
    fn plot(&self, top: f64) -> NSRect {
        rect(PLOT_LEFT, top + PLOT_TOP, (self.bounds().size.width - PLOT_LEFT).max(1.0), PLOT_HEIGHT)
    }

    /// The bar at `at`: the pointer anywhere in its column of the plot.
    fn bar_at(&self, at: NSPoint) -> Option<(Plot, usize)> {
        let stats = self.ivars().stats.borrow();
        let stats = stats.as_ref()?;
        [(Plot::Due, DUE_TOP, stats.due_by_day.len()), (Plot::Reviews, REVIEWS_TOP, stats.reviews_by_day.len())].into_iter().find_map(|(plot, top, bars)| {
            let area = self.plot(top);
            let inside = bars > 0 && at.x >= area.origin.x && at.x < area.origin.x + area.size.width && at.y >= area.origin.y - 6.0 && at.y <= area.origin.y + area.size.height + 18.0;
            inside.then(|| (plot, (((at.x - area.origin.x) / area.size.width * bars as f64) as usize).min(bars - 1)))
        })
    }

    fn draw(&self) {
        let stats = self.ivars().stats.borrow();
        let Some(stats) = stats.as_ref() else { return };
        let width = self.bounds().size.width;
        let colours = series(dark());
        let title = NSFont::systemFontOfSize_weight(13.0, unsafe { NSFontWeightSemibold });
        let small = NSFont::systemFontOfSize(11.0);
        let (ink, quiet, faint) = (NSColor::labelColor(), NSColor::secondaryLabelColor(), NSColor::tertiaryLabelColor());
        draw_text(&self.ivars().heading.borrow(), 0.0, 0.0, &NSFont::systemFontOfSize(18.0), &ink, Align::Left);

        // The cards: one bar, a part a kind, and what each part is under it.
        draw_text(strings::CHART_CARDS, 0.0, CARDS_TOP, &title, &ink, Align::Left);
        draw_text(&strings::chart_cards(stats.cards), width, CARDS_TOP + 2.0, &small, &quiet, Align::Right);
        let kinds = [(strings::CHART_NEW, stats.fresh), (strings::CHART_YOUNG, stats.young), (strings::CHART_MATURE, stats.mature)];
        let bar = rect(0.0, CARDS_TOP + 26.0, width, 12.0);
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bar, 4.0, 4.0).addClip();
        let shown: Vec<usize> = kinds.iter().enumerate().filter(|(_, kind)| kind.1 > 0).map(|(i, _)| i).collect();
        // The surface shows between the parts: 2 points each.
        let room = width - 2.0 * shown.len().saturating_sub(1) as f64;
        let mut x = 0.0;
        for &i in &shown {
            let part = room * kinds[i].1 as f64 / stats.cards.max(1) as f64;
            colours[i].setFill();
            NSBezierPath::fillRect(rect(x, bar.origin.y, part, bar.size.height));
            x += part + 2.0;
        }
        NSGraphicsContext::restoreGraphicsState_class();
        let mut x = 0.0;
        for (i, (name, cards)) in kinds.iter().enumerate() {
            x = self.legend(x, CARDS_TOP + 48.0, &colours[i], &format!("{name} {cards} · {} %", (cards * 100 + stats.cards / 2) / stats.cards.max(1)), &small, &quiet) + 18.0;
        }

        // What comes due: a bar a day, today first.
        let due = &stats.due_by_day;
        let coming = |days: usize| due.iter().skip(1).take(days).sum::<usize>();
        let pointed = self.ivars().hover.get();
        draw_text(strings::CHART_DUE, 0.0, DUE_TOP, &title, &ink, Align::Left);
        let says = match pointed {
            Some((Plot::Due, day)) => strings::chart_due_day(day, due[day]),
            _ => strings::chart_due_sums(coming(1), coming(7), due.len().saturating_sub(1), coming(due.len())),
        };
        draw_text(&says, width, DUE_TOP + 2.0, &small, &quiet, Align::Right);
        let area = self.plot(DUE_TOP);
        let top = scale_top(due.iter().copied().max().unwrap_or(0));
        self.grid(area, top, &small, &faint);
        let slot = area.size.width / due.len().max(1) as f64;
        for (day, &cards) in due.iter().enumerate() {
            let x = area.origin.x + day as f64 * slot;
            if pointed == Some((Plot::Due, day)) {
                NSColor::quaternaryLabelColor().setFill();
                NSBezierPath::fillRect(rect(x, area.origin.y, slot, area.size.height));
            }
            let height = (area.size.height * cards as f64 / top as f64).max(if cards > 0 { 2.0 } else { 0.0 });
            colours[0].setFill();
            fill_bar(rect(x + 1.0, area.origin.y + area.size.height - height, (slot - 2.0).max(1.0), height), true);
            if day % 10 == 0 {
                // The first and the last stay inside the plot.
                let (label, at, align) = match day {
                    0 => (strings::CHART_TODAY.to_owned(), area.origin.x, Align::Left),
                    _ if day + 1 == due.len() => (strings::chart_days_ahead(day), area.origin.x + area.size.width, Align::Right),
                    _ => (strings::chart_days_ahead(day), x + slot / 2.0, Align::Middle),
                };
                draw_text(&label, at, area.origin.y + area.size.height + 4.0, &small, &faint, align);
            }
        }
        if due.iter().all(|cards| *cards == 0) {
            draw_text(strings::CHART_NOTHING_DUE, area.origin.x + area.size.width / 2.0, area.origin.y + area.size.height / 2.0 - 8.0, &small, &faint, Align::Middle);
        }

        // The reviews: a bar a day, today last; the forgotten over the recalled.
        let past = &stats.reviews_by_day;
        let (all, again) = past.iter().fold((0, 0), |(all, again), day| (all + day.0, again + day.1));
        draw_text(&strings::chart_reviews(past.len()), 0.0, REVIEWS_TOP, &title, &ink, Align::Left);
        let says = match pointed {
            Some((Plot::Reviews, day)) => strings::chart_reviews_day(past.len() - 1 - day, past[day].0, past[day].1),
            _ => strings::chart_reviews_sums(all, again),
        };
        draw_text(&says, width, REVIEWS_TOP + 2.0, &small, &quiet, Align::Right);
        let area = self.plot(REVIEWS_TOP);
        let top = scale_top(past.iter().map(|day| day.0).max().unwrap_or(0));
        self.grid(area, top, &small, &faint);
        let slot = area.size.width / past.len().max(1) as f64;
        let base = area.origin.y + area.size.height;
        for (day, &(reviews, forgotten)) in past.iter().enumerate() {
            let x = area.origin.x + day as f64 * slot;
            if pointed == Some((Plot::Reviews, day)) {
                NSColor::quaternaryLabelColor().setFill();
                NSBezierPath::fillRect(rect(x, area.origin.y, slot, area.size.height));
            }
            let high = |n: usize| (area.size.height * n as f64 / top as f64).max(if n > 0 { 2.0 } else { 0.0 });
            let (recalled, forgotten) = (high(reviews - forgotten), high(forgotten));
            let bar = (slot - 2.0).max(1.0);
            colours[0].setFill();
            fill_bar(rect(x + 1.0, base - recalled, bar, recalled), forgotten == 0.0);
            // The surface shows between the two.
            let lift = if recalled > 0.0 { recalled + 2.0 } else { 0.0 };
            colours[1].setFill();
            fill_bar(rect(x + 1.0, base - lift - forgotten, bar, forgotten), true);
            let ago = past.len() - 1 - day;
            if ago % 10 == 0 || day == 0 {
                let (label, at, align) = match ago {
                    0 => (strings::CHART_TODAY.to_owned(), area.origin.x + area.size.width, Align::Right),
                    _ if day == 0 => (strings::chart_days_ago(ago), area.origin.x, Align::Left),
                    _ => (strings::chart_days_ago(ago), x + slot / 2.0, Align::Middle),
                };
                draw_text(&label, at, base + 4.0, &small, &faint, align);
            }
        }
        if all == 0 {
            draw_text(strings::CHART_NO_REVIEWS, area.origin.x + area.size.width / 2.0, area.origin.y + area.size.height / 2.0 - 8.0, &small, &faint, Align::Middle);
        }
        let x = self.legend(PLOT_LEFT, base + 26.0, &colours[0], strings::CHART_RECALLED, &small, &quiet) + 18.0;
        self.legend(x, base + 26.0, &colours[1], strings::CHART_FORGOTTEN, &small, &quiet);

        // The grades: a row a grade, its bar as long as its share.
        draw_text(&strings::chart_grades(past.len()), 0.0, GRADES_TOP, &title, &ink, Align::Left);
        let most = stats.grades.iter().copied().max().unwrap_or(0).max(1);
        let graded: usize = stats.grades.iter().sum();
        let room = (width - GRADE_LEFT - 80.0).max(1.0);
        for (row, (name, &reviews)) in strings::GRADES.iter().zip(&stats.grades).enumerate() {
            let y = GRADES_TOP + 28.0 + row as f64 * GRADE_ROW;
            draw_text(name, 0.0, y, &small, &quiet, Align::Left);
            let long = (room * reviews as f64 / most as f64).max(if reviews > 0 { 2.0 } else { 0.0 });
            colours[0].setFill();
            let bar = rect(GRADE_LEFT, y + 2.0, long, 10.0);
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bar, 3.0f64.min(long / 2.0), 3.0f64.min(long / 2.0)).fill();
            if long > 3.0 {
                NSBezierPath::fillRect(rect(GRADE_LEFT, y + 2.0, long - 3.0, 10.0));
            }
            let share = (reviews * 100 + graded / 2).checked_div(graded).map(|share| format!(" · {share} %")).unwrap_or_default();
            draw_text(&format!("{reviews}{share}"), GRADE_LEFT + long + 6.0, y, &small, &quiet, Align::Left);
        }
    }

    /// A plot's lines and their numbers: its base, its top, and the middle when it is whole.
    fn grid(&self, area: NSRect, top: usize, font: &NSFont, colour: &NSColor) {
        let lines: &[usize] = if top.is_multiple_of(2) { &[0, 1, 2] } else { &[0, 2] };
        for &half in lines {
            let y = (area.origin.y + area.size.height * (1.0 - half as f64 / 2.0)).round();
            NSColor::separatorColor().setFill();
            NSBezierPath::fillRect(rect(area.origin.x, y - if half == 0 { 0.0 } else { 0.5 }, area.size.width, if half == 0 { 1.0 } else { 0.5 }));
            draw_text(&(top * half / 2).to_string(), area.origin.x - 6.0, y - 7.0, font, colour, Align::Right);
        }
    }

    /// A legend's entry at `x`: its colour's mark and its words; where it ends.
    fn legend(&self, x: f64, y: f64, colour: &NSColor, words: &str, font: &NSFont, ink: &NSColor) -> f64 {
        colour.setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(x, y + 3.0, 8.0, 8.0), 2.0, 2.0).fill();
        draw_text(words, x + 13.0, y, font, ink, Align::Left);
        x + 13.0 + text_width(words, font)
    }
}
