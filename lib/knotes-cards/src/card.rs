use std::ops::Range;

/// How a card asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    /// A question and its answer (`question::answer`, or the two apart on a `?` line).
    Basic,
    /// A text with words left out (`==words==`), all of them asked at once.
    Cloze,
}

/// A card as its note writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Its deck, a path: `мови/англійська`.
    pub deck: String,
    /// Its lasting id (`c-a1b2c3`, written `^c-a1b2c3` at its end); none until it is given one.
    pub id: Option<String>,
    pub kind: CardKind,
    /// What is shown first: the question, or the text with `[…]` for the words left out.
    pub front: String,
    /// What answers it: the answer, or the text whole.
    pub back: String,
    /// Where the card is in its note's text (bytes), its deck mark to its id.
    pub span: Range<usize>,
}
