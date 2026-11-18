//! The review of a deck's cards (ADR 0037), over the note column: how many cards wait and a
//! button to start; then a card's question, its answer once asked for, and four grades, each
//! saying when the card would come back. Space shows the answer, 1 to 4 grade it.

use std::cell::RefCell;

use knotes_cards::{CardLink, Grade, text_runs};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBox, NSBoxType, NSButton, NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSLayoutConstraint, NSLineBreakMode, NSLinkAttributeName, NSScrollView, NSStackView,
    NSTextDelegate, NSTextField, NSTextView, NSTextViewDelegate, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSAttributedString, NSDictionary, NSMutableAttributedString, NSObject, NSObjectProtocol, NSSize, NSString};

use crate::strings;

/// What a button of the review asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Start,
    /// The deck's cards not due yet too (the owner's «повторювати колоду» out of schedule).
    StartAhead,
    ShowAnswer,
    Grade(Grade),
    /// The note the card is written in.
    OpenNote,
    /// The last grade taken back, its card shown again.
    Undo,
    /// A link in the card's text, clicked.
    Link(CardLink),
}

type OnAsked = Box<dyn Fn(Asked)>;

#[derive(Default)]
pub struct TargetIvars {
    on_asked: RefCell<Option<OnAsked>>,
}

define_class!(
    /// The buttons' target.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesReviewTarget"]
    #[ivars = TargetIvars]
    struct ReviewTarget;

    unsafe impl NSObjectProtocol for ReviewTarget {}

    unsafe impl NSTextDelegate for ReviewTarget {}

    unsafe impl NSTextViewDelegate for ReviewTarget {
        /// A link in the card's text: a note's (`note:target`) or the web's.
        #[unsafe(method(textView:clickedOnLink:atIndex:))]
        fn clicked_on_link(&self, _view: &NSTextView, link: &AnyObject, _index: usize) -> bool {
            let link = link.downcast_ref::<NSString>().map(|s| s.to_string());
            if let Some(link) = &link {
                self.ask(Asked::Link(match link.strip_prefix(NOTE_LINK) {
                    Some(target) => CardLink::Note(target.to_owned()),
                    None => CardLink::Web(link.clone()),
                }));
            }
            link.is_some()
        }
    }

    impl ReviewTarget {
        #[unsafe(method(reviewAhead:))]
        fn review_ahead(&self, _sender: Option<&AnyObject>) {
            self.ask(Asked::StartAhead);
        }

        #[unsafe(method(reviewStart:))]
        fn start(&self, _sender: Option<&AnyObject>) {
            self.ask(Asked::Start);
        }

        #[unsafe(method(reviewShow:))]
        fn show(&self, _sender: Option<&AnyObject>) {
            self.ask(Asked::ShowAnswer);
        }

        #[unsafe(method(reviewGrade:))]
        fn grade(&self, sender: &NSButton) {
            if let Some(grade) = Grade::from_number(sender.tag() as i64) {
                self.ask(Asked::Grade(grade));
            }
        }

        #[unsafe(method(reviewOpen:))]
        fn open(&self, _sender: Option<&AnyObject>) {
            self.ask(Asked::OpenNote);
        }

        #[unsafe(method(reviewUndo:))]
        fn undo(&self, _sender: Option<&AnyObject>) {
            self.ask(Asked::Undo);
        }
    }
);

impl ReviewTarget {
    fn ask(&self, asked: Asked) {
        if let Some(callback) = self.ivars().on_asked.borrow().as_ref() {
            callback(asked);
        }
    }
}

/// What a link to a note starts with, as the text keeps it.
const NOTE_LINK: &str = "note:";

/// What the review shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// A deck chosen: its name, how many cards wait (`more`: at least that many), not started;
    /// under it the deck's statistics, as text (empty for none).
    /// `ahead`: how many reviewed cards are not due yet (`ahead_more`: at least that many).
    Waiting { deck: String, cards: usize, more: bool, stats: String, charts: Option<knotes_core::DeckStats>, ahead: usize, ahead_more: bool },
    /// A card's question; with `answer`, its answer and the grades, each with when the card
    /// would come back (Again, Hard, Good, Easy).
    Card { deck: String, done: usize, left: usize, front: String, answer: Option<(String, [String; 4])> },
    /// The deck's cards are reviewed: how many were.
    Done { deck: String, reviewed: usize },
}

pub struct ReviewView {
    pub view: Retained<NSView>,
    header: Retained<NSTextField>,
    /// The card's question and, under it, its answer: text that scrolls, its links clicked.
    text: Retained<NSTextView>,
    /// The text's scroll view, and the one of a waiting deck's charts that takes its place.
    text_scroll: Retained<NSScrollView>,
    charts_scroll: Retained<NSScrollView>,
    charts: Retained<crate::deck_charts::DeckCharts>,
    start: Retained<NSButton>,
    /// Beside it: the deck's cards not due yet too.
    ahead: Retained<NSButton>,
    starts: Retained<NSStackView>,
    show: Retained<NSButton>,
    grades: Retained<NSStackView>,
    grade_buttons: [Retained<NSButton>; 4],
    open: Retained<NSButton>,
    /// Takes the last grade back (⌘Z), while there is one of this review to take back.
    undo: Retained<NSButton>,
    links: Retained<NSStackView>,
    target: Retained<ReviewTarget>,
    shown: RefCell<Option<Shown>>,
}

impl ReviewView {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let plate = NSBox::new(mtm);
        plate.setBoxType(NSBoxType::Custom);
        plate.setBorderWidth(0.0);
        plate.setCornerRadius(0.0);
        plate.setFillColor(&NSColor::textBackgroundColor());
        plate.setContentViewMargins(NSSize::new(0.0, 0.0));
        let view: Retained<NSView> = Retained::into_super(plate);
        view.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.setHidden(true);
        let target: Retained<ReviewTarget> = unsafe { msg_send![super(ReviewTarget::alloc(mtm).set_ivars(TargetIvars::default())), init] };
        let text = |size: f64, colour: &NSColor| {
            let field = NSTextField::wrappingLabelWithString(&NSString::new(), mtm);
            field.setFont(Some(&NSFont::systemFontOfSize(size)));
            field.setTextColor(Some(colour));
            field.setSelectable(true);
            field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
            field
        };
        let button = |title: &str, action, key: &str| {
            // SAFETY: the view keeps its target as long as its buttons.
            let button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(&target), Some(action), mtm) };
            button.setKeyEquivalent(&NSString::from_str(key));
            button
        };
        let header = text(NSFont::smallSystemFontSize(), &NSColor::secondaryLabelColor());
        let scroll: Retained<NSScrollView> = NSTextView::scrollableTextView(mtm);
        scroll.setDrawsBackground(false);
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        let card: Retained<NSTextView> = scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).expect("a scrollable text view holds one");
        card.setEditable(false);
        card.setSelectable(true);
        card.setDrawsBackground(false);
        card.setTextContainerInset(NSSize::new(0.0, 0.0));
        // Flush with the line above it and the buttons below.
        // SAFETY: the text view's own container.
        if let Some(container) = unsafe { card.textContainer() } {
            container.setLineFragmentPadding(0.0);
        }
        card.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*target)));
        // A waiting deck's charts, where the card's text is: as wide as it, scrolled when taller.
        let charts = crate::deck_charts::DeckCharts::new(mtm);
        charts.setTranslatesAutoresizingMaskIntoConstraints(false);
        let charts_scroll = NSScrollView::new(mtm);
        charts_scroll.setDrawsBackground(false);
        charts_scroll.setHasVerticalScroller(true);
        charts_scroll.setAutohidesScrollers(true);
        charts_scroll.setDocumentView(Some(&charts));
        charts_scroll.setHidden(true);
        let start = button(strings::REVIEW_START, sel!(reviewStart:), "\r");
        let show = button(strings::REVIEW_SHOW_ANSWER, sel!(reviewShow:), " ");
        let grade_buttons = [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy].map(|grade| {
            let number = grade as i32;
            let button = button("", sel!(reviewGrade:), &number.to_string());
            button.setTag(number as isize);
            button
        });
        let grade_views: Vec<Retained<NSView>> = grade_buttons.iter().map(|b| Retained::into_super(Retained::into_super(b.clone()))).collect();
        let grades = NSStackView::stackViewWithViews(&NSArray::from_retained_slice(&grade_views), mtm);
        grades.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        grades.setSpacing(8.0);
        let open = button(strings::REVIEW_OPEN_NOTE, sel!(reviewOpen:), "");
        open.setBordered(false);
        open.setContentTintColor(Some(&NSColor::linkColor()));
        // The buttons under the text, at the column's bottom.
        let ahead = button("", sel!(reviewAhead:), "");
        ahead.setToolTip(Some(&NSString::from_str(strings::REVIEW_AHEAD_HELP)));
        let both: [&NSView; 2] = [&start, &ahead];
        let starts = NSStackView::stackViewWithViews(&NSArray::from_slice(&both), mtm);
        starts.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        starts.setSpacing(8.0);
        let undo = button(strings::REVIEW_UNDO, sel!(reviewUndo:), "z");
        undo.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::Command);
        undo.setBordered(false);
        undo.setContentTintColor(Some(&NSColor::linkColor()));
        undo.setToolTip(Some(&NSString::from_str(strings::REVIEW_UNDO_HELP)));
        undo.setHidden(true);
        let quiet: [&NSView; 2] = [&open, &undo];
        let links = NSStackView::stackViewWithViews(&NSArray::from_slice(&quiet), mtm);
        links.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        links.setSpacing(18.0);
        let rows: [&NSView; 4] = [&starts, &show, &grades, &links];
        let controls = NSStackView::stackViewWithViews(&NSArray::from_slice(&rows), mtm);
        controls.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        controls.setAlignment(objc2_app_kit::NSLayoutAttribute::Leading);
        controls.setSpacing(12.0);
        let column = NSView::new(mtm);
        for part in [&*header as &NSView, &scroll, &charts_scroll, &controls, &column] {
            part.setTranslatesAutoresizingMaskIntoConstraints(false);
        }
        for part in [&*header as &NSView, &scroll, &charts_scroll, &controls] {
            column.addSubview(part);
        }
        let clip = charts_scroll.contentView();
        view.addSubview(&column);
        // As wide as the note column lets it, 620 points at most. Tied to the column's width, not
        // a width of its own: a wish for 620 points widened the column (and the window) even
        // while hidden. Weaker than the split view holds its columns (250): stronger, it kept
        // the note column from growing with the window.
        let wide = column.widthAnchor().constraintEqualToAnchor_constant(&view.widthAnchor(), -64.0);
        wide.setPriority(240.0);
        header.setContentHuggingPriority_forOrientation(1.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
        let constraints = [
            column.topAnchor().constraintEqualToAnchor_constant(&view.safeAreaLayoutGuide().topAnchor(), 40.0),
            column.bottomAnchor().constraintEqualToAnchor_constant(&view.bottomAnchor(), -28.0),
            column.centerXAnchor().constraintEqualToAnchor(&view.centerXAnchor()),
            column.leadingAnchor().constraintGreaterThanOrEqualToAnchor_constant(&view.leadingAnchor(), 32.0),
            column.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -32.0),
            wide,
            column.widthAnchor().constraintLessThanOrEqualToConstant(620.0),
            header.topAnchor().constraintEqualToAnchor(&column.topAnchor()),
            header.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            header.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            scroll.topAnchor().constraintEqualToAnchor_constant(&header.bottomAnchor(), 14.0),
            scroll.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            scroll.trailingAnchor().constraintEqualToAnchor(&column.trailingAnchor()),
            scroll.bottomAnchor().constraintEqualToAnchor_constant(&controls.topAnchor(), -16.0),
            charts_scroll.topAnchor().constraintEqualToAnchor(&scroll.topAnchor()),
            charts_scroll.leadingAnchor().constraintEqualToAnchor(&scroll.leadingAnchor()),
            charts_scroll.trailingAnchor().constraintEqualToAnchor(&scroll.trailingAnchor()),
            charts_scroll.bottomAnchor().constraintEqualToAnchor(&scroll.bottomAnchor()),
            charts.topAnchor().constraintEqualToAnchor(&clip.topAnchor()),
            charts.leadingAnchor().constraintEqualToAnchor(&clip.leadingAnchor()),
            charts.trailingAnchor().constraintEqualToAnchor(&clip.trailingAnchor()),
            charts.heightAnchor().constraintEqualToConstant(crate::deck_charts::HEIGHT),
            controls.leadingAnchor().constraintEqualToAnchor(&column.leadingAnchor()),
            controls.trailingAnchor().constraintLessThanOrEqualToAnchor(&column.trailingAnchor()),
            controls.bottomAnchor().constraintEqualToAnchor(&column.bottomAnchor()),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        Self { view, header, text: card, text_scroll: scroll, charts_scroll, charts, start, ahead, starts, show, grades, grade_buttons, open, undo, links, target, shown: RefCell::new(None) }
    }

    /// Called with what a button asks for.
    pub fn on_asked(&self, callback: impl Fn(Asked) + 'static) {
        *self.target.ivars().on_asked.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `shown`; hidden with none.
    pub fn show(&self, shown: Option<Shown>) {
        self.view.setHidden(shown.is_none());
        let (card, answered, waiting) = match &shown {
            Some(Shown::Waiting { deck, cards, more, stats, charts, ahead, ahead_more }) => {
                self.ahead.setTitle(&NSString::from_str(&strings::review_ahead(*ahead, *ahead_more)));
                self.header.setStringValue(&NSString::from_str(deck));
                let waits = if *cards == 0 { strings::REVIEW_NOTHING.to_owned() } else { strings::review_waiting(*cards, *more) };
                // The charts say it above themselves; without them, the text.
                match charts {
                    Some(charts) => self.charts.set(&waits, charts.clone(), stats),
                    None => self.set_text(&waits, None, Some(stats).filter(|s| !s.is_empty())),
                }
                (false, false, *cards > 0)
            }
            Some(Shown::Card { deck, done, left, front, answer }) => {
                self.header.setStringValue(&NSString::from_str(&strings::review_progress(deck, *done, *left)));
                self.set_text(front, answer.as_ref().map(|(back, _)| back.as_str()), None);
                if let Some((_, comes_back)) = answer {
                    for ((button, name), when) in self.grade_buttons.iter().zip(strings::GRADES).zip(comes_back) {
                        button.setTitle(&NSString::from_str(&format!("{name} · {when}")));
                    }
                }
                (true, answer.is_some(), false)
            }
            Some(Shown::Done { deck, reviewed }) => {
                self.header.setStringValue(&NSString::from_str(deck));
                self.set_text(&strings::review_done(*reviewed), None, None);
                (false, false, false)
            }
            None => (false, false, false),
        };
        let charts = matches!(&shown, Some(Shown::Waiting { charts: Some(_), .. }));
        self.charts_scroll.setHidden(!charts);
        self.text_scroll.setHidden(charts);
        let ahead = matches!(&shown, Some(Shown::Waiting { ahead, .. }) if *ahead > 0);
        self.start.setHidden(!waiting);
        self.ahead.setHidden(!ahead);
        self.starts.setHidden(!waiting && !ahead);
        self.show.setHidden(!card || answered);
        self.grades.setHidden(!answered);
        self.open.setHidden(!card);
        // With what is shown changed, no grade to take back until the window says there is.
        self.undo.setHidden(true);
        self.links.setHidden(!card);
        *self.shown.borrow_mut() = shown;
    }

    /// Whether a grade of this review can be taken back: its button shows then.
    pub fn set_undo(&self, there: bool) {
        self.undo.setHidden(!there);
        self.links.setHidden(self.open.isHidden() && !there);
    }

    /// The text shown: `front`, and under a rule `back` when there is one; their links (wiki and
    /// Markdown ones) as links to click.
    fn set_text(&self, front: &str, back: Option<&str>, small: Option<&String>) {
        let whole = NSMutableAttributedString::new();
        let piece = |text: &str, size: f64, colour: &NSColor, link: Option<&CardLink>| {
            let font = NSFont::systemFontOfSize(size);
            // SAFETY: AppKit's attribute name constants live for the whole process.
            let (font_key, colour_key, link_key) = unsafe { (NSFontAttributeName, NSForegroundColorAttributeName, NSLinkAttributeName) };
            let mut keys = vec![font_key, colour_key];
            let mut values: Vec<Retained<AnyObject>> = vec![Retained::into_super(Retained::into_super(font)), Retained::into_super(Retained::into_super(colour.retain()))];
            if let Some(link) = link {
                let address = match link {
                    CardLink::Note(target) => format!("{NOTE_LINK}{target}"),
                    CardLink::Web(address) => address.clone(),
                };
                keys.push(link_key);
                values.push(Retained::into_super(Retained::into_super(NSString::from_str(&address))));
            }
            let attributes = NSDictionary::from_retained_objects(&keys, &values);
            // SAFETY: a font, a colour and a string are those attributes' value types.
            let run = unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(text), Some(&attributes)) };
            whole.appendAttributedString(&run);
        };
        let label = NSColor::labelColor();
        for run in text_runs(front) {
            piece(&run.text, 18.0, &label, run.link.as_ref());
        }
        if let Some(back) = back {
            piece("\n\n———\n\n", 12.0, &NSColor::tertiaryLabelColor(), None);
            for run in text_runs(back) {
                piece(&run.text, 16.0, &label, run.link.as_ref());
            }
        }
        // A deck's statistics: small and quiet under what waits.
        if let Some(small) = small {
            piece(&format!("\n\n{small}"), 13.0, &NSColor::secondaryLabelColor(), None);
        }
        // SAFETY: the text view's own storage.
        if let Some(storage) = unsafe { self.text.textStorage() } {
            storage.setAttributedString(&whole);
        }
        self.text.scrollPoint(objc2_foundation::NSPoint::new(0.0, 0.0));
    }

    /// A click on the `n`th link of the text shown (probe mode).
    pub fn click_link(&self, n: usize) {
        let shown = self.shown.borrow().clone();
        let Some(Shown::Card { front, answer, .. }) = shown else { return };
        let back = answer.map(|(back, _)| back).unwrap_or_default();
        let link = text_runs(&front).into_iter().chain(text_runs(&back)).filter_map(|run| run.link).nth(n);
        if let Some(link) = link {
            self.target.ask(Asked::Link(link));
        }
    }

    /// What it shows, for the probe: `waiting мови 3`, `question мови 0/3 | cat`,
    /// `answer мови 0/3 | cat | кіт | Знову · 10 хв, …`, `done мови 3`, or `hidden`.
    pub fn describe(&self) -> String {
        if self.view.isHidden() {
            return "hidden".to_owned();
        }
        match &*self.shown.borrow() {
            Some(Shown::Waiting { deck, cards, more, .. }) => format!("waiting {deck} {cards}{}", if *more { "+" } else { "" }),
            Some(Shown::Card { deck, done, left, front, answer: None }) => format!("question {deck} {done}/{left} | {front}"),
            Some(Shown::Card { deck, done, left, front, answer: Some((back, _)) }) => {
                let grades: Vec<String> = self.grade_buttons.iter().map(|b| b.title().to_string()).collect();
                format!("answer {deck} {done}/{left} | {front} | {back} | {}", grades.join(", "))
            }
            Some(Shown::Done { deck, reviewed }) => format!("done {deck} {reviewed}"),
            None => "hidden".to_owned(),
        }
    }

    /// The waiting deck's statistics as shown (probe mode); empty for none.
    pub fn describe_stats(&self) -> String {
        match &*self.shown.borrow() {
            Some(Shown::Waiting { stats, .. }) => stats.clone(),
            _ => String::new(),
        }
    }

    /// A click on the button that asks for `asked` (probe mode); nothing when it is hidden.
    pub fn click(&self, asked: Asked) {
        let button = match asked {
            Asked::Start => &self.start,
            Asked::StartAhead => &self.ahead,
            Asked::ShowAnswer => &self.show,
            Asked::Grade(grade) => &self.grade_buttons[grade as usize - 1],
            Asked::OpenNote => &self.open,
            Asked::Undo => &self.undo,
            Asked::Link(_) => return,
        };
        if !button.isHiddenOrHasHiddenAncestor() {
            // SAFETY: a click on the view's own button.
            unsafe { button.performClick(None) };
        }
    }
}
