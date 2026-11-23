//! The calendar of daily notes, in the right panel: a month in weeks from Monday, today filled
//! in the accent colour and the days that have a note written in it. A click on a day asks for
//! its note; the arrows step by a month, «Сьогодні» comes back to this one and to today's note.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use knotes_app::{Day, month_after, month_cells};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSColor, NSFont, NSFontAttributeName, NSFontWeightRegular, NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage, NSLayoutConstraintOrientation, NSStackView, NSStackViewDistribution, NSTextAlignment,
    NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSAttributedString, NSDictionary, NSObject, NSObjectProtocol, NSString};

use crate::strings;

/// The weeks a month may take.
const WEEKS: usize = 6;
/// A day's height, in points.
const DAY_HEIGHT: f64 = 28.0;

type OnDay = Box<dyn Fn(Day)>;

pub struct Ivars {
    /// The month shown: its year and month.
    month: Cell<(i32, u8)>,
    today: Cell<Day>,
    /// The days that have a note.
    noted: RefCell<HashSet<Day>>,
    title: RefCell<Option<Retained<NSTextField>>>,
    /// The days' buttons, week by week; a button's tag is its day of the month.
    days: RefCell<Vec<Retained<NSButton>>>,
    on_day: RefCell<Option<OnDay>>,
}

define_class!(
    /// The calendar's state and its buttons' target.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesCalendar"]
    #[ivars = Ivars]
    pub struct Calendar;

    unsafe impl NSObjectProtocol for Calendar {}

    impl Calendar {
        /// An arrow: the month before (its tag -1) or after (1).
        #[unsafe(method(stepMonth:))]
        fn step_month(&self, sender: &NSButton) {
            let (year, month) = self.ivars().month.get();
            self.ivars().month.set(month_after(year, month, sender.tag() as i32));
            self.fill();
        }

        /// «Сьогодні»: this month again, and today's note, as a click on the day asks for it.
        #[unsafe(method(showToday:))]
        fn show_today(&self, _sender: Option<&AnyObject>) {
            let today = self.ivars().today.get();
            self.ivars().month.set((today.0, today.1));
            self.fill();
            if let Some(callback) = self.ivars().on_day.borrow().as_ref() {
                callback(today);
            }
        }

        /// A click on a day.
        #[unsafe(method(dayClicked:))]
        fn day_clicked(&self, sender: &NSButton) {
            let (year, month) = self.ivars().month.get();
            if let (Ok(day @ 1..), Some(callback)) = (u8::try_from(sender.tag()), self.ivars().on_day.borrow().as_ref()) {
                callback((year, month, day));
            }
        }
    }
);

impl Calendar {
    /// The month's name and its days, as they are now.
    fn fill(&self) {
        let ivars = self.ivars();
        let (year, month) = ivars.month.get();
        if let Some(title) = ivars.title.borrow().as_ref() {
            title.setStringValue(&NSString::from_str(&strings::month_of(year, month)));
        }
        let cells = month_cells(year, month);
        let noted = ivars.noted.borrow();
        for (i, button) in ivars.days.borrow().iter().enumerate() {
            let day = cells.get(i).copied().flatten();
            button.setTag(day.map_or(0, isize::from));
            button.setEnabled(day.is_some());
            let today = day.is_some_and(|d| (year, month, d) == ivars.today.get());
            let has_note = day.is_some_and(|d| noted.contains(&(year, month, d)));
            // Today filled in the accent colour; a day with a note written in it.
            let colour = if today {
                NSColor::whiteColor()
            } else if has_note {
                NSColor::controlAccentColor()
            } else {
                NSColor::labelColor()
            };
            // SAFETY: a font weight constant.
            let weight = unsafe { if today || has_note { NSFontWeightSemibold } else { NSFontWeightRegular } };
            let font = NSFont::systemFontOfSize_weight(NSFont::systemFontSize(), weight);
            let text = NSString::from_str(&day.map(|d| d.to_string()).unwrap_or_default());
            // SAFETY: AppKit's attribute names, each with a value of its kind.
            let title = unsafe {
                let values: [&AnyObject; 2] = [font.as_ref(), colour.as_ref()];
                let attributes = NSDictionary::from_slices(&[NSFontAttributeName, NSForegroundColorAttributeName], &values);
                NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &text, Some(&attributes))
            };
            button.setAttributedTitle(&title);
            button.setToolTip(day.filter(|_| has_note).map(|_| NSString::from_str(strings::CALENDAR_HAS_NOTE)).as_deref());
            if let Some(layer) = button.layer() {
                let fill = if today { NSColor::controlAccentColor() } else { NSColor::clearColor() };
                layer.setBackgroundColor(Some(&fill.CGColor()));
            }
        }
    }
}

pub struct CalendarPanel {
    pub view: Retained<NSView>,
    calendar: Retained<Calendar>,
}

impl CalendarPanel {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let calendar = Calendar::alloc(mtm).set_ivars(Ivars {
            month: Cell::new((1970, 1)),
            today: Cell::new((1970, 1, 1)),
            noted: RefCell::new(HashSet::new()),
            title: RefCell::new(None),
            days: RefCell::new(Vec::new()),
            on_day: RefCell::new(None),
        });
        let calendar: Retained<Calendar> = unsafe { msg_send![super(calendar), init] };
        let row = |views: &[&NSView], equal: bool| {
            let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(views), mtm);
            stack.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
            stack.setSpacing(2.0);
            if equal {
                stack.setDistribution(NSStackViewDistribution::FillEqually);
            }
            stack
        };
        let arrow = |symbol: &str, words: &str, by: isize| {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), Some(&NSString::from_str(words))).unwrap_or_else(NSImage::new);
            // SAFETY: the calendar outlives its buttons (the panel keeps both) and takes the action.
            let button = unsafe { NSButton::buttonWithImage_target_action(&image, Some(&calendar), Some(sel!(stepMonth:)), mtm) };
            button.setBordered(false);
            button.setTag(by);
            button.setToolTip(Some(&NSString::from_str(words)));
            button
        };
        let (before, after) = (arrow("chevron.left", strings::CALENDAR_BEFORE, -1), arrow("chevron.right", strings::CALENDAR_AFTER, 1));
        let title = NSTextField::labelWithString(&NSString::new(), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize())));
        // The name takes the room between the arrows and «Сьогодні».
        title.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
        // SAFETY: as the arrows.
        let today = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::TODAY), Some(&calendar), Some(sel!(showToday:)), mtm) };
        today.setControlSize(objc2_app_kit::NSControlSize::Small);
        let head = row(&[&title, &today, &before, &after], false);
        head.setSpacing(8.0);
        let names: Vec<Retained<NSTextField>> = strings::WEEKDAYS
            .iter()
            .map(|name| {
                let label = NSTextField::labelWithString(&NSString::from_str(name), mtm);
                label.setAlignment(NSTextAlignment::Center);
                label.setTextColor(Some(&NSColor::secondaryLabelColor()));
                label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
                label
            })
            .collect();
        let names: Vec<&NSView> = names.iter().map(|label| -> &NSView { label }).collect();
        let mut rows: Vec<Retained<NSStackView>> = vec![head, row(&names, true)];
        let mut days: Vec<Retained<NSButton>> = Vec::new();
        for _ in 0..WEEKS {
            let week: Vec<Retained<NSButton>> = (0..7)
                .map(|_| {
                    // SAFETY: as the arrows.
                    let day = unsafe { NSButton::buttonWithTitle_target_action(&NSString::new(), Some(&calendar), Some(sel!(dayClicked:)), mtm) };
                    day.setBordered(false);
                    day.setWantsLayer(true);
                    if let Some(layer) = day.layer() {
                        layer.setCornerRadius(6.0);
                    }
                    day.heightAnchor().constraintEqualToConstant(DAY_HEIGHT).setActive(true);
                    day
                })
                .collect();
            let views: Vec<&NSView> = week.iter().map(|day| -> &NSView { day }).collect();
            rows.push(row(&views, true));
            days.extend(week);
        }
        let rows: Vec<&NSView> = rows.iter().map(|row| -> &NSView { row }).collect();
        let month = NSStackView::stackViewWithViews(&NSArray::from_slice(&rows), mtm);
        month.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        month.setSpacing(2.0);
        month.setTranslatesAutoresizingMaskIntoConstraints(false);
        let view = NSView::new(mtm);
        view.addSubview(&month);
        let guide = view.safeAreaLayoutGuide();
        let mut constraints = vec![
            month.topAnchor().constraintEqualToAnchor_constant(&guide.topAnchor(), 12.0),
            month.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 12.0),
            month.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -12.0),
        ];
        // Each row as wide as the month.
        constraints.extend(rows.iter().map(|row| row.widthAnchor().constraintEqualToAnchor(&month.widthAnchor())));
        for c in constraints {
            c.setActive(true);
        }
        *calendar.ivars().title.borrow_mut() = Some(title);
        *calendar.ivars().days.borrow_mut() = days;
        Self { view, calendar }
    }

    /// The day it is and the days that have a note. The month shown stays, but while it is the
    /// one of the day it was: then the calendar goes on to today's with the date.
    pub fn show(&self, today: Day, noted: HashSet<Day>) {
        let ivars = self.calendar.ivars();
        let was = ivars.today.replace(today);
        if ivars.month.get() == (was.0, was.1) {
            ivars.month.set((today.0, today.1));
        }
        *ivars.noted.borrow_mut() = noted;
        self.calendar.fill();
    }

    /// Called with the day clicked.
    pub fn on_day(&self, callback: impl Fn(Day) + 'static) {
        *self.calendar.ivars().on_day.borrow_mut() = Some(Box::new(callback));
    }

    /// `before`, `after` or `today`: that button clicked; `day=5`: that day of the month shown
    /// (probe mode).
    pub fn click(&self, what: &str) {
        let ivars = self.calendar.ivars();
        let button = match what.strip_prefix("day=").and_then(|d| d.parse::<isize>().ok()) {
            Some(day) => ivars.days.borrow().iter().find(|b| b.tag() == day).cloned(),
            None => {
                let by = match what {
                    "before" => -1,
                    "after" => 1,
                    "today" => 0,
                    _ => return,
                };
                // The head's buttons, by what they do.
                let head = self.view.subviews().firstObject().and_then(|month| month.subviews().firstObject());
                let buttons = head.map(|head| head.subviews().iter().filter_map(|v| v.downcast::<NSButton>().ok()).collect::<Vec<_>>()).unwrap_or_default();
                buttons.into_iter().find(|b| if by == 0 { b.action() == Some(sel!(showToday:)) } else { b.action() == Some(sel!(stepMonth:)) && b.tag() == by })
            }
        };
        if let Some(button) = button {
            // SAFETY: a click on the calendar's own button.
            unsafe { button.performClick(None) };
        }
    }

    /// `Листопад 2026 | today 22 | notes 3 21`: the month shown, today when it is in it, and its
    /// days drawn as having a note (probe mode).
    pub fn describe(&self) -> String {
        let ivars = self.calendar.ivars();
        let (year, month) = ivars.month.get();
        let today = ivars.today.get();
        let noted = ivars.noted.borrow();
        let days: Vec<String> = ivars.days.borrow().iter().filter_map(|b| u8::try_from(b.tag()).ok().filter(|d| noted.contains(&(year, month, *d)))).map(|d| d.to_string()).collect();
        let title = ivars.title.borrow().as_ref().map(|t| t.stringValue().to_string()).unwrap_or_default();
        format!("{title} | today {} | notes {}", if (today.0, today.1) == (year, month) { today.2.to_string() } else { "—".to_owned() }, days.join(" "))
    }
}
