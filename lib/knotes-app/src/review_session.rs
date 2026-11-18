//! A deck's review under way (ADR 0037): which card is asked, what waits, where each card
//! graded stands, and the last grade while it can be taken back. No window, no files and no
//! clock here: the caller reads the notes, writes the log and says what time it is.

use std::collections::{HashMap, HashSet, VecDeque};

use knotes_cards::{Card, DeckSettings, Fsrs, Grade, Memory, Schedule, scan_cards};
use knotes_core::{CardState, DueCard};

/// The card shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardAsked {
    pub card: DueCard,
    pub front: String,
    pub back: String,
    /// Its answer shows.
    pub answered: bool,
}

/// A grade about to be given: what the log is told (`at`, `id`), and what the review keeps
/// once it is told (`ReviewSession::graded`).
#[derive(Debug, Clone, PartialEq)]
pub struct Graded {
    /// When it is logged (seconds since 1970).
    pub at: i64,
    /// The card's id.
    pub id: String,
    /// Where the card stood before it (none: a new card), and stands after it.
    pub before: Option<Schedule>,
    pub after: Schedule,
}

/// A grade as it was given: what taking it back puts right.
#[derive(Debug, Clone)]
struct LastGrade {
    asked: CardAsked,
    at: i64,
    before: Option<Schedule>,
    /// Still learned after it, the card was put at the queue's end.
    pushed: bool,
}

/// A deck's review under way.
#[derive(Debug, Clone)]
pub struct ReviewSession {
    deck: String,
    queue: VecDeque<DueCard>,
    current: Option<CardAsked>,
    reviewed: usize,
    /// Where each card graded in this review stands after its grades, by its id. The index
    /// learns of a grade only at the sync after it is logged: asked of the index, a card that
    /// came back at once looked new again, was sent to its first step again, and so on at
    /// every Good, each one logged.
    graded: HashMap<String, Schedule>,
    /// The last grade given, while it can be taken back (⌘Z): one step, as far as a slip of
    /// the finger goes.
    last: Option<LastGrade>,
}

/// Where a card would stand graded `grade` at `at`, from where it stands now (`before`; none:
/// a new card), by its deck's chance of recall and steps.
pub fn would_stand(settings: &DeckSettings, deck: &str, before: Option<&Schedule>, at: i64, grade: Grade) -> Schedule {
    let fsrs = Fsrs { retention: settings.retention(deck), ..Fsrs::default() };
    knotes_cards::review(&fsrs, &settings.steps(deck), before, at, grade)
}

/// `card` as its note's `text` holds it now: by its id, or where it was; none if it is gone.
/// Of two with its id in the note, the one where the index found it (the first was shown for
/// both).
pub fn card_written(text: &str, card: &DueCard) -> Option<Card> {
    let cards = scan_cards(text);
    let at = |c: &&Card| c.span.start == card.start as usize;
    match &card.id {
        Some(id) => {
            let with_id = || cards.iter().filter(|c| c.id.as_ref() == Some(id));
            with_id().find(at).or_else(|| with_id().next()).cloned()
        }
        None => cards.iter().filter(|c| c.id.is_none()).find(at).cloned(),
    }
}

impl ReviewSession {
    /// A review of `deck`: the cards `due`, then those `ahead` of the schedule. An id is one
    /// card's, with one schedule: written at two cards (a line copied, a note duplicated), the
    /// first is asked and the other not, a second time over. No card is asked yet (`next`).
    pub fn start(deck: &str, due: Vec<DueCard>, ahead: Vec<DueCard>) -> Self {
        let mut ids = HashSet::new();
        let queue = due.into_iter().chain(ahead).filter(|card| card.id.as_ref().is_none_or(|id| ids.insert(id.clone()))).collect();
        Self { deck: deck.to_owned(), queue, current: None, reviewed: 0, graded: HashMap::new(), last: None }
    }

    pub fn deck(&self) -> &str {
        &self.deck
    }

    /// Grades given, those taken back not counted.
    pub fn reviewed(&self) -> usize {
        self.reviewed
    }

    /// Cards still to grade, the one shown among them.
    pub fn left(&self) -> usize {
        self.queue.len() + usize::from(self.current.is_some())
    }

    /// The card shown; none before the first `next`, and when the review is done.
    pub fn current(&self) -> Option<&CardAsked> {
        self.current.as_ref()
    }

    /// Whether a grade can be taken back.
    pub fn can_undo(&self) -> bool {
        self.last.is_some()
    }

    /// The next card that is still written where the index found it becomes the one shown;
    /// `read` gives a note's text. None left, the review is done.
    pub fn next(&mut self, read: impl Fn(&str) -> Option<String>) {
        self.current = None;
        while let Some(card) = self.queue.pop_front() {
            if let Some(written) = read(&card.path).and_then(|text| card_written(&text, &card)) {
                let card = DueCard { start: written.span.start as u32, end: written.span.end as u32, ..card };
                self.current = Some(CardAsked { card, front: written.front, back: written.back, answered: false });
                break;
            }
        }
    }

    /// The answer of the card shown is shown.
    pub fn show_answer(&mut self) {
        if let Some(current) = self.current.as_mut() {
            current.answered = true;
        }
    }

    /// Where the card `id` stands now: as this review left it when it graded it, else as the
    /// index has it (`index`); none for a card never reviewed.
    pub fn standing(&self, id: &str, index: impl FnOnce(&str) -> Option<CardState>) -> Option<Schedule> {
        self.graded.get(id).copied().or_else(|| index(id).map(|s| Schedule::standing(Memory { stability: s.stability, difficulty: s.difficulty }, s.last, s.step, s.relearning)))
    }

    /// Where the card shown would stand after each grade (Again, Hard, Good, Easy) given at
    /// `now`: what the grades' buttons say. None while its answer does not show.
    pub fn would_come_back(&self, settings: &DeckSettings, now: i64, index: impl FnOnce(&str) -> Option<CardState>) -> Option<[Schedule; 4]> {
        let current = self.current.as_ref().filter(|c| c.answered)?;
        let before = current.card.id.as_deref().and_then(|id| self.standing(id, index));
        Some([Grade::Again, Grade::Hard, Grade::Good, Grade::Easy].map(|grade| would_stand(settings, &current.card.deck, before.as_ref(), now, grade)))
    }

    /// The card shown was given `id`, written at `at` of its note, `added` bytes long. The cards
    /// of that note still waiting stand further on by it: one with no id is found by its
    /// place, and was passed over as gone (a note of twenty typed cards gave one a review).
    pub fn id_given(&mut self, id: String, at: usize, added: usize) {
        let Some(current) = self.current.as_mut() else { return };
        for waiting in self.queue.iter_mut().filter(|w| w.path == current.card.path && w.start as usize >= at) {
            waiting.start += added as u32;
            waiting.end += added as u32;
        }
        current.card.end += added as u32;
        current.card.id = Some(id);
    }

    /// The card shown graded `grade` at `now`, not kept yet: the caller logs it first, and a
    /// log that cannot be written leaves the review as it was. None unless its answer shows
    /// and it has an id (`id_given`). Logged a second later than its last grade at least: the
    /// index tells a card's reviews apart by their second, and one asked again at once was
    /// graded twice in one.
    pub fn grade(&self, settings: &DeckSettings, now: i64, grade: Grade, index: impl FnOnce(&str) -> Option<CardState>) -> Option<Graded> {
        let current = self.current.as_ref().filter(|c| c.answered)?;
        let id = current.card.id.clone()?;
        let before = self.standing(&id, index);
        let at = before.map_or(now, |before| now.max(before.last + 1));
        let after = would_stand(settings, &current.card.deck, before.as_ref(), at, grade);
        Some(Graded { at, id, before, after })
    }

    /// The grade is logged: kept. Still learned after it, the card comes back at the review's
    /// end. The caller shows the next card (`next`).
    pub fn graded(&mut self, graded: Graded) {
        let Some(asked) = self.current.clone() else { return };
        self.reviewed += 1;
        self.graded.insert(graded.id, graded.after);
        let pushed = graded.after.step.is_some();
        if pushed {
            self.queue.push_back(asked.card.clone());
        }
        self.last = Some(LastGrade { asked, at: graded.at, before: graded.before, pushed });
    }

    /// The last grade, while it can be taken back: when it was logged, and its card.
    pub fn last_grade(&self) -> Option<(i64, &DueCard)> {
        self.last.as_ref().map(|last| (last.at, &last.asked.card))
    }

    /// The last grade taken back (the caller has said so in the log): its card shows again
    /// with its answer, to be graded as meant, and the card that was shown waits its turn
    /// again.
    pub fn take_back(&mut self) {
        let Some(last) = self.last.take() else { return };
        let id = last.asked.card.id.clone().unwrap_or_default();
        self.reviewed = self.reviewed.saturating_sub(1);
        // Where it stood before the grade, as this review knows it.
        match last.before {
            Some(before) => self.graded.insert(id.clone(), before),
            None => self.graded.remove(&id),
        };
        // Out of the queue's end if the grade put it there.
        if last.pushed
            && let Some(at) = self.queue.iter().rposition(|c| c.id.as_deref() == Some(id.as_str()))
        {
            self.queue.remove(at);
        }
        if let Some(shown) = self.current.take() {
            self.queue.push_front(shown.card);
        }
        self.current = Some(CardAsked { answered: true, ..last.asked });
    }
}
