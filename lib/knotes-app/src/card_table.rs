//! A deck's cards as a table (ADR 0037): each card where it is written and how it stands,
//! sorted by a column, and a card's text changed in its note. No window here: the table's
//! window draws these rows and writes what `replace_block` gives.

use std::cmp::Ordering;

use knotes_cards::Card;
use knotes_core::CardRow;

/// A column of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardColumn {
    /// The card's question: sorted by, the cards stand as their notes have them.
    Question,
    Deck,
    Due,
    /// The days between its last review and the next.
    Interval,
    Reviews,
    Lapses,
}

/// How a card stands, said short in its row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Standing {
    /// Paused: asked no more until it is resumed, however it stood.
    Paused,
    /// Never reviewed.
    New,
    /// At one of its steps, due at this time.
    Learning(i64),
    /// Its days have begun: due at this time, the days between its last review and that.
    Review { due: i64, days: f64 },
}

const DAY: f64 = 86_400.0;

pub fn standing(row: &CardRow) -> Standing {
    match row.reviewed {
        _ if row.paused => Standing::Paused,
        None => Standing::New,
        Some(reviewed) if reviewed.state.step.is_some() => Standing::Learning(reviewed.due),
        Some(reviewed) => Standing::Review { due: reviewed.due, days: (reviewed.due - reviewed.state.last) as f64 / DAY },
    }
}

/// The cards of a table, in the order of the column they are sorted by; those a search keeps,
/// while one is typed.
#[derive(Debug, Default)]
pub struct CardTable {
    rows: Vec<CardRow>,
    sorted: Option<(CardColumn, bool)>,
    /// Every card, while a search shows a part of them in `rows`.
    all: Option<Vec<CardRow>>,
}

/// What a search in the table keeps: the cards whose deck's name holds every word typed (any
/// case), and those written where the words were found (`found`: a note's path, and the place
/// in it from where to where: the cards found to hold them).
#[derive(Debug, Default)]
pub struct CardSearch {
    words: Vec<String>,
    found: std::collections::HashMap<String, Vec<(u32, u32)>>,
}

impl CardSearch {
    pub fn new(query: &str, found: impl IntoIterator<Item = (String, u32, u32)>) -> Self {
        let mut by_note: std::collections::HashMap<String, Vec<(u32, u32)>> = std::collections::HashMap::new();
        for (path, start, end) in found {
            by_note.entry(path).or_default().push((start, end));
        }
        Self { words: query.split_whitespace().map(str::to_lowercase).collect(), found: by_note }
    }

    /// Whether the search keeps `row`: nothing typed keeps every card.
    pub fn keeps(&self, row: &CardRow) -> bool {
        if self.words.is_empty() {
            return true;
        }
        let deck = row.deck.to_lowercase();
        self.words.iter().all(|word| deck.contains(word)) || self.found.get(&row.path).is_some_and(|places| places.iter().any(|(start, end)| *start < row.end && row.start < *end))
    }
}

impl CardTable {
    /// `rows` as the index gives them: by note, in the note's order.
    pub fn new(rows: Vec<CardRow>) -> Self {
        Self { rows, sorted: None, all: None }
    }

    /// How many cards the table has, whatever a search shows of them.
    pub fn count(&self) -> usize {
        self.all.as_ref().map_or(self.rows.len(), Vec::len)
    }

    /// Shows the cards `search` keeps, in the order the table is sorted by (as their notes
    /// have them, when by none).
    pub fn search(&mut self, search: &CardSearch) {
        let all = self.all.take().unwrap_or_else(|| std::mem::take(&mut self.rows));
        self.rows = all.iter().filter(|row| search.keeps(row)).cloned().collect();
        // Every card kept: no second copy of them.
        self.all = (self.rows.len() < all.len()).then_some(all);
        if let Some((column, up)) = self.sorted {
            self.sort(column, up);
        }
    }

    pub fn rows(&self) -> &[CardRow] {
        &self.rows
    }

    /// The column sorted by and whether upwards; none while the cards stand as their notes
    /// have them.
    pub fn sorted(&self) -> Option<(CardColumn, bool)> {
        self.sorted
    }

    /// Sorts by `column`: upwards, or the other way when it is the column sorted by already.
    /// Cards that are the same there keep their notes' order. A card never reviewed has no
    /// due day, interval or reviews: it stands after those that have, either way.
    pub fn sort_by(&mut self, column: CardColumn) {
        let up = !matches!(self.sorted, Some((by, true)) if by == column);
        self.sort(column, up);
    }

    /// The rows given anew (the vault changed), in the order the table is sorted by.
    pub fn reload(&mut self, rows: Vec<CardRow>) {
        // Every card shows again: a search under way is made anew by who asked for it.
        self.all = None;
        self.rows = rows;
        if let Some((column, up)) = self.sorted {
            self.sort(column, up);
        }
    }

    fn sort(&mut self, column: CardColumn, up: bool) {
        // By note first: the sort below is stable.
        self.rows.sort_by(|a, b| (a.path.as_str(), a.start).cmp(&(b.path.as_str(), b.start)));
        let number = |row: &CardRow| -> Option<f64> {
            let reviewed = row.reviewed?;
            Some(match column {
                CardColumn::Due => reviewed.due as f64,
                CardColumn::Interval => (reviewed.due - reviewed.state.last) as f64,
                CardColumn::Reviews => f64::from(reviewed.reviews),
                CardColumn::Lapses => f64::from(reviewed.lapses),
                CardColumn::Question | CardColumn::Deck => 0.0,
            })
        };
        let turned = |order: Ordering| if up { order } else { order.reverse() };
        match column {
            CardColumn::Question => {
                if !up {
                    self.rows.reverse();
                }
            }
            CardColumn::Deck => self.rows.sort_by(|a, b| turned(crate::natural_cmp(&a.deck, &b.deck))),
            _ => self.rows.sort_by(|a, b| match (number(a), number(b)) {
                (Some(a), Some(b)) => turned(a.total_cmp(&b)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }),
        }
        self.sorted = Some((column, up));
    }

    /// The card written at `start` of the note `path` had its text changed there, from `was`
    /// bytes long to `now`: it ends where it ends now, and the cards after it in that note
    /// stand further on (or back) by the difference. The index says the same a moment later;
    /// until then a card with no id, found by its place, would not be found.
    pub fn block_changed(&mut self, path: &str, start: u32, was: usize, now: usize) {
        let by = now as i64 - was as i64;
        let moved = |at: u32| (i64::from(at) + by).max(0) as u32;
        for row in self.rows.iter_mut().chain(self.all.iter_mut().flatten()).filter(|row| row.path == path && row.start >= start) {
            if row.start > start {
                row.start = moved(row.start);
            }
            row.end = moved(row.end);
        }
    }

    /// The card `like` paused or resumed, and given `id` if it had none (a card is paused by
    /// its id): said in its row at once, before the index hears of it.
    pub fn set_paused(&mut self, like: &CardRow, id: &str, paused: bool) {
        for row in self.rows.iter_mut().chain(self.all.iter_mut().flatten()).filter(|row| row.path == like.path && row.start == like.start) {
            row.paused = paused;
            row.id = Some(id.to_owned());
        }
    }

    /// The card `like` was put in the deck `deck`: its row says so.
    pub fn set_deck(&mut self, like: &CardRow, deck: &str) {
        for row in self.rows.iter_mut().chain(self.all.iter_mut().flatten()).filter(|row| row.path == like.path && row.start == like.start) {
            row.deck = deck.to_owned();
        }
    }

    /// The card `like` is gone from its note: its row goes.
    pub fn remove(&mut self, like: &CardRow) {
        let gone = |row: &CardRow| row.path == like.path && row.start == like.start;
        self.rows.retain(|row| !gone(row));
        if let Some(all) = self.all.as_mut() {
            all.retain(|row| !gone(row));
        }
    }

    /// Where the card `like` is in the table now: by its id, or (with none) by its note and
    /// its place there. To keep the row chosen across a sort or a reload.
    pub fn position(&self, like: &CardRow) -> Option<usize> {
        match &like.id {
            Some(id) => self.rows.iter().position(|row| row.id.as_ref() == Some(id) && row.path == like.path),
            None => self.rows.iter().position(|row| row.id.is_none() && row.path == like.path && row.start == like.start),
        }
    }
}

/// The table's rows chosen together (a click with ⌘ or ⇧, ⇧ with an arrow, ⌘A), beside the
/// one whose card shows below: by their places, so they last until the rows are put anew (a
/// sort, a search, the vault changed). Empty while one row alone is chosen.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarkedRows {
    rows: std::collections::BTreeSet<usize>,
    /// Where ⇧ with an arrow goes on from.
    lead: Option<usize>,
}

impl MarkedRows {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn holds(&self, row: usize) -> bool {
        self.rows.contains(&row)
    }

    /// Every row chosen, the one at `chosen` among them, from the top.
    pub fn with(&self, chosen: Option<usize>) -> Vec<usize> {
        if self.rows.is_empty() { chosen.into_iter().collect() } else { self.rows.iter().copied().collect() }
    }

    /// A click with ⌘ on `row`: chosen too, or no more if it was (the last one stays). Gives
    /// the row whose card shows now: the one clicked, or with it gone the first still chosen.
    pub fn toggle(&mut self, chosen: Option<usize>, row: usize) -> usize {
        let Some(chosen) = chosen else { return row };
        self.rows.insert(chosen);
        self.lead = None;
        let shown = if !self.rows.insert(row) {
            if self.rows.len() > 1 {
                self.rows.remove(&row);
            }
            if row == chosen { self.rows.first().copied().unwrap_or(row) } else { chosen }
        } else {
            row
        };
        if self.rows.len() < 2 {
            self.rows.clear();
        }
        shown
    }

    /// A click with ⇧ on `row`: every row from the one at `chosen` to it.
    pub fn extend(&mut self, chosen: usize, row: usize) {
        self.rows = if row == chosen { Default::default() } else { (chosen.min(row)..=chosen.max(row)).collect() };
        self.lead = Some(row);
    }

    /// ⇧ with an arrow: the rows chosen reach a row further down (or up) from `chosen`, of
    /// `len` rows. Gives the row reached.
    pub fn step(&mut self, chosen: usize, down: bool, len: usize) -> usize {
        let lead = self.lead.unwrap_or(chosen);
        let to = if down { (lead + 1).min(len.saturating_sub(1)) } else { lead.saturating_sub(1) };
        self.extend(chosen, to);
        to
    }

    /// ⌘A: all `len` rows.
    pub fn all(&mut self, len: usize) {
        self.rows = if len > 1 { (0..len).collect() } else { Default::default() };
        self.lead = None;
    }
}

/// The card of `row` among its note's `cards` (as `scan_cards` gives them): by its id (of two
/// with it, the one where the index found it), or with none by its place.
pub fn card_of<'a>(cards: &'a [Card], row: &CardRow) -> Option<&'a Card> {
    let at = |card: &&Card| card.span.start == row.start as usize;
    match &row.id {
        Some(id) => {
            let with_id = || cards.iter().filter(|card| card.id.as_ref() == Some(id));
            with_id().find(at).or_else(|| with_id().next())
        }
        None => cards.iter().filter(|card| card.id.is_none()).find(at),
    }
}

/// A card's `block` (its text in its note, from its deck's mark on) put in the deck `deck`:
/// the mark's deck replaced, the rest as it was. None when the block does not begin with a
/// deck's mark, or `deck` is no deck's name (empty, or with a space in it).
pub fn in_deck(block: &str, deck: &str) -> Option<String> {
    let deck = deck.trim().trim_matches('/');
    let rest = block.strip_prefix("#@")?;
    // The mark's deck ends at a space, a brace (a card in braces) or the block's end.
    let end = rest.find(|c: char| c.is_whitespace() || c == '{').unwrap_or(rest.len());
    (!deck.is_empty() && end > 0 && !deck.contains(|c: char| c.is_whitespace() || c == '{')).then(|| format!("#@{deck}{}", &rest[end..]))
}

/// What goes from the note's `text` with the card's block at `block` deleted: the block, and
/// where it is a line of its own that line's end, and of a blank line on each side of it one
/// (at the note's end, the one before it). A card inside a line takes one of the two spaces
/// around it. So a deleted card leaves no gap where it was.
pub fn block_to_delete(text: &str, block: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let (Some(before), Some(after)) = (text.get(..block.start), text.get(block.end..)) else { return block };
    let (mut start, mut end) = (block.start, block.end);
    if (before.is_empty() || before.ends_with('\n')) && (after.is_empty() || after.starts_with('\n')) {
        end += usize::from(!after.is_empty());
        if before.is_empty() || before.ends_with("\n\n") {
            if text[end..].starts_with('\n') {
                end += 1;
            } else if end == text.len() && !before.is_empty() {
                start -= 1;
            }
        }
    } else if before.ends_with(' ') && after.starts_with(' ') {
        end += 1;
    }
    start..end
}

/// The note's `text` with the card's block `old` (found at `start` when the table was filled)
/// replaced by `new`. The note may have changed since: the block is taken where it stands now
/// (at `start`, else where the note holds it, if it holds it once). None when it is not there
/// as it was, or is there twice: nothing is written over text the table did not show.
pub fn replace_block(text: &str, start: usize, old: &str, new: &str) -> Option<String> {
    if old.is_empty() {
        return None;
    }
    let at = if text.get(start..).is_some_and(|rest| rest.starts_with(old)) {
        start
    } else {
        let mut found = text.match_indices(old).map(|(at, _)| at);
        let first = found.next()?;
        if found.next().is_some() {
            return None;
        }
        first
    };
    Some(format!("{}{new}{}", &text[..at], &text[at + old.len()..]))
}
