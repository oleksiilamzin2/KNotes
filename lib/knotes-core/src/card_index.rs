use crate::BoxError;

/// A deck's own cards (not those of the decks under it), counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckCount {
    /// The deck's path: `мови/англійська`.
    pub deck: String,
    pub cards: usize,
    /// Reviewed before and due again.
    pub due: usize,
    /// Never reviewed.
    pub fresh: usize,
}

/// A card to review: where it is written, and when it was due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueCard {
    /// Its note, and where the card is in the note's text (bytes).
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub deck: String,
    /// Its lasting id; none for a card not given one yet.
    pub id: Option<String>,
    /// When it became due (seconds since 1970); none for a card never reviewed.
    pub due: Option<i64>,
}

/// What is remembered of a reviewed card: FSRS's stability (days) and difficulty (1 to 10), and
/// when it was last reviewed (seconds since 1970).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardState {
    pub stability: f64,
    pub difficulty: f64,
    pub last: i64,
}

/// A card where it is written, with where it stands (for the tools' lists).
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    /// Its note, and where the card is in the note's text (bytes).
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub deck: String,
    /// Its lasting id; none for a card not given one yet.
    pub id: Option<String>,
    /// None for a card never reviewed.
    pub reviewed: Option<Reviewed>,
}

/// A reviewed card's schedule: what is remembered of it, when it is due again (seconds since
/// 1970), how many times it was reviewed and how many of them forgotten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reviewed {
    pub state: CardState,
    /// Its first review: the day it was new.
    pub first: i64,
    pub due: i64,
    pub reviews: u32,
    pub lapses: u32,
}

/// How a deck with those under it stands (for its statistics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckStats {
    pub cards: usize,
    /// Never reviewed.
    pub fresh: usize,
    /// Reviewed, and due again within `MATURE_DAYS` of the last review; the rest are mature.
    pub young: usize,
    pub mature: usize,
    /// The reviewed cards due each day from today on (`FORECAST_DAYS` and today): today's with
    /// those overdue first.
    pub due_by_day: Vec<usize>,
    /// Each of the last `PAST_DAYS` days, today last: its reviews, and how many of them were
    /// graded Again.
    pub reviews_by_day: Vec<(usize, usize)>,
    /// Those days' reviews by grade: Again, Hard, Good, Easy.
    pub grades: [usize; 4],
}

/// A card whose interval is this many days or more is mature (Anki's measure).
pub const MATURE_DAYS: i64 = 21;
pub const FORECAST_DAYS: usize = 30;
pub const PAST_DAYS: usize = 30;

/// The cards the vault's notes hold (ADR 0037), their decks, and what is due.
pub trait CardIndex {
    /// Every deck with its own cards counted, those due by `until` (seconds since 1970) and
    /// those never reviewed; sorted by deck.
    fn deck_counts(&self, until: i64) -> Result<Vec<DeckCount>, BoxError>;
    /// The cards to review by `until` in `deck` and the decks under it (every deck for none):
    /// those due, the longest due first, then those never reviewed in their notes' order;
    /// `limit` at most, and of those never reviewed `new` at most (the day's new cards).
    fn due_cards(&self, deck: Option<&str>, until: i64, limit: usize, new: usize) -> Result<Vec<DueCard>, BoxError>;
    /// The reviewed cards of `deck` and the decks under it not due by `after` yet, the soonest
    /// due first, `limit` at most: what a review ahead of the schedule asks.
    fn cards_ahead(&self, deck: &str, after: i64, limit: usize) -> Result<Vec<DueCard>, BoxError>;
    /// The decks with cards first reviewed at `since` or later (seconds since 1970), each with
    /// how many of its own: the new cards a day has taken already.
    fn introduced(&self, since: i64) -> Result<Vec<(String, usize)>, BoxError>;
    /// The notes holding a card of `deck` or of a deck under it, sorted by path.
    fn deck_notes(&self, deck: &str) -> Result<Vec<String>, BoxError>;
    /// Every card of `deck` and the decks under it (of every deck for none), in the note `path`
    /// only when given; by note, then in the note's order.
    fn card_rows(&self, deck: Option<&str>, path: Option<&str>) -> Result<Vec<CardRow>, BoxError>;
    /// How `deck` with those under it stands; `day_start`: when today began (seconds since 1970).
    fn deck_stats(&self, deck: &str, day_start: i64) -> Result<DeckStats, BoxError>;
    /// Where the card `id` stands; none for one never reviewed.
    fn card_state(&self, id: &str) -> Result<Option<CardState>, BoxError>;
}
