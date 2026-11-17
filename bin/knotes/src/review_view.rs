//! The review of a deck's cards (ADR 0037), over the note column: how many cards wait and a
//! button to start; then a card's question, its answer once asked for, and four grades, each
//! saying when the card would come back. Space shows the answer, 1 to 4 grade it.

use std::cell::RefCell;

use knotes_cards::Grade;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSBox, NSBoxType, NSButton, NSColor, NSFont, NSLayoutConstraint, NSLineBreakMode, NSStackView, NSTextField, NSUserInterfaceLayoutOrientation, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSSize, NSString};

use crate::strings;

/// What a button of the review asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Start,
    ShowAnswer,
    Grade(Grade),
    /// The note the card is written in.
    OpenNote,
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

    impl ReviewTarget {
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
    }
);

impl ReviewTarget {
    fn ask(&self, asked: Asked) {
        if let Some(callback) = self.ivars().on_asked.borrow().as_ref() {
            callback(asked);
        }
    }
}

/// What the review shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// A deck chosen: its name, how many cards wait (`more`: at least that many), not started.
    Waiting { deck: String, cards: usize, more: bool },
    /// A card's question; with `answer`, its answer and the grades, each with when the card
    /// would come back (Again, Hard, Good, Easy).
    Card { deck: String, done: usize, left: usize, front: String, answer: Option<(String, [String; 4])> },
    /// The deck's cards are reviewed: how many were.
    Done { deck: String, reviewed: usize },
}

pub struct ReviewView {
    pub view: Retained<NSView>,
    header: Retained<NSTextField>,
    front: Retained<NSTextField>,
    line: Retained<NSBox>,
    back: Retained<NSTextField>,
    start: Retained<NSButton>,
    show: Retained<NSButton>,
    grades: Retained<NSStackView>,
    grade_buttons: [Retained<NSButton>; 4],
    open: Retained<NSButton>,
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
        let front = text(18.0, &NSColor::labelColor());
        let back = text(16.0, &NSColor::labelColor());
        let line = NSBox::new(mtm);
        line.setBoxType(NSBoxType::Separator);
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
        let rows: [&NSView; 8] = [&header, &front, &line, &back, &start, &show, &grades, &open];
        let column = NSStackView::stackViewWithViews(&NSArray::from_slice(&rows), mtm);
        column.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        column.setAlignment(objc2_app_kit::NSLayoutAttribute::Leading);
        column.setSpacing(14.0);
        column.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&column);
        let wide = column.widthAnchor().constraintEqualToConstant(620.0);
        // Wished for, never forced: stronger than the split view holds its columns, it widened the
        // note column (and the window) even while hidden.
        wide.setPriority(100.0);
        let constraints = [
            column.topAnchor().constraintEqualToAnchor_constant(&view.safeAreaLayoutGuide().topAnchor(), 40.0),
            column.centerXAnchor().constraintEqualToAnchor(&view.centerXAnchor()),
            column.leadingAnchor().constraintGreaterThanOrEqualToAnchor_constant(&view.leadingAnchor(), 32.0),
            column.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -32.0),
            wide,
            front.widthAnchor().constraintEqualToAnchor(&column.widthAnchor()),
            back.widthAnchor().constraintEqualToAnchor(&column.widthAnchor()),
            line.widthAnchor().constraintEqualToAnchor(&column.widthAnchor()),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        Self { view, header, front, line, back, start, show, grades, grade_buttons, open, target, shown: RefCell::new(None) }
    }

    /// Called with what a button asks for.
    pub fn on_asked(&self, callback: impl Fn(Asked) + 'static) {
        *self.target.ivars().on_asked.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows `shown`; hidden with none.
    pub fn show(&self, shown: Option<Shown>) {
        self.view.setHidden(shown.is_none());
        let set = |field: &NSTextField, text: &str| field.setStringValue(&NSString::from_str(text));
        let (card, answered, waiting) = match &shown {
            Some(Shown::Waiting { deck, cards, more }) => {
                set(&self.header, deck);
                set(&self.front, &if *cards == 0 { strings::REVIEW_NOTHING.to_owned() } else { strings::review_waiting(*cards, *more) });
                (false, false, *cards > 0)
            }
            Some(Shown::Card { deck, done, left, front, answer }) => {
                set(&self.header, &strings::review_progress(deck, *done, *left));
                set(&self.front, front);
                if let Some((back, comes_back)) = answer {
                    set(&self.back, back);
                    for ((button, name), when) in self.grade_buttons.iter().zip(strings::GRADES).zip(comes_back) {
                        button.setTitle(&NSString::from_str(&format!("{name} · {when}")));
                    }
                }
                (true, answer.is_some(), false)
            }
            Some(Shown::Done { deck, reviewed }) => {
                set(&self.header, deck);
                set(&self.front, &strings::review_done(*reviewed));
                (false, false, false)
            }
            None => (false, false, false),
        };
        self.start.setHidden(!waiting);
        self.show.setHidden(!card || answered);
        self.line.setHidden(!answered);
        self.back.setHidden(!answered);
        self.grades.setHidden(!answered);
        self.open.setHidden(!card);
        *self.shown.borrow_mut() = shown;
    }

    /// What it shows, for the probe: `waiting мови 3`, `question мови 0/3 | cat`,
    /// `answer мови 0/3 | cat | кіт | Знову · 10 хв, …`, `done мови 3`, or `hidden`.
    pub fn describe(&self) -> String {
        if self.view.isHidden() {
            return "hidden".to_owned();
        }
        match &*self.shown.borrow() {
            Some(Shown::Waiting { deck, cards, more }) => format!("waiting {deck} {cards}{}", if *more { "+" } else { "" }),
            Some(Shown::Card { deck, done, left, front, answer: None }) => format!("question {deck} {done}/{left} | {front}"),
            Some(Shown::Card { deck, done, left, front, answer: Some((back, _)) }) => {
                let grades: Vec<String> = self.grade_buttons.iter().map(|b| b.title().to_string()).collect();
                format!("answer {deck} {done}/{left} | {front} | {back} | {}", grades.join(", "))
            }
            Some(Shown::Done { deck, reviewed }) => format!("done {deck} {reviewed}"),
            None => "hidden".to_owned(),
        }
    }

    /// A click on the button that asks for `asked` (probe mode); nothing when it is hidden.
    pub fn click(&self, asked: Asked) {
        let button = match asked {
            Asked::Start => &self.start,
            Asked::ShowAnswer => &self.show,
            Asked::Grade(grade) => &self.grade_buttons[grade as usize - 1],
            Asked::OpenNote => &self.open,
        };
        if !button.isHiddenOrHasHiddenAncestor() {
            // SAFETY: a click on the view's own button.
            unsafe { button.performClick(None) };
        }
    }
}
