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

/// The cards the vault's notes hold (ADR 0037), their decks, and what is due.
pub trait CardIndex {
    /// Every deck with its own cards counted, those due by `until` (seconds since 1970) and
    /// those never reviewed; sorted by deck.
    fn deck_counts(&self, until: i64) -> Result<Vec<DeckCount>, BoxError>;
    /// The cards to review by `until` in `deck` and the decks under it (every deck for none):
    /// those due, the longest due first, then those never reviewed in their notes' order;
    /// `limit` at most.
    fn due_cards(&self, deck: Option<&str>, until: i64, limit: usize) -> Result<Vec<DueCard>, BoxError>;
    /// The notes holding a card of `deck` or of a deck under it, sorted by path.
    fn deck_notes(&self, deck: &str) -> Result<Vec<String>, BoxError>;
}
