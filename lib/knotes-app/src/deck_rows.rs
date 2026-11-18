//! A part of the vault's cards (a space's, ADR 0030 and 0037), counted and queued from their
//! rows as the index does for all of them: the index knows no spaces, the window gives it the
//! rows of the space's notes.

use std::collections::BTreeMap;

use knotes_core::{CardRow, DeckCount, DueCard};

/// Whether the row's card is of `deck` or a deck under it; every card is of the deck `""`,
/// which is all of them.
fn under(row: &CardRow, deck: &str) -> bool {
    deck.is_empty() || row.deck == deck || row.deck.strip_prefix(deck).is_some_and(|rest| rest.starts_with('/'))
}

fn due_card(row: &CardRow) -> DueCard {
    DueCard { path: row.path.clone(), start: row.start, end: row.end, deck: row.deck.clone(), id: row.id.clone(), due: row.reviewed.map(|s| s.due) }
}

/// Every deck of `rows` with its own cards counted, those due by `until` and those never
/// reviewed; sorted by deck (as `CardIndex::deck_counts`).
pub fn deck_counts(rows: &[CardRow], until: i64) -> Vec<DeckCount> {
    let mut decks: BTreeMap<&str, DeckCount> = BTreeMap::new();
    for row in rows {
        let count = decks.entry(&row.deck).or_insert_with(|| DeckCount { deck: row.deck.clone(), cards: 0, due: 0, fresh: 0 });
        count.cards += 1;
        match row.reviewed {
            // A card paused is the deck's, but neither due nor new: it does not wait.
            _ if row.paused => {}
            None => count.fresh += 1,
            Some(s) if s.due <= until => count.due += 1,
            Some(_) => {}
        }
    }
    decks.into_values().collect()
}

/// The decks of `rows` with cards first reviewed at `since` or later, each with how many of
/// its own (as `CardIndex::introduced`).
pub fn introduced(rows: &[CardRow], since: i64) -> Vec<(String, usize)> {
    let mut decks: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.reviewed.is_some_and(|s| s.first >= since)) {
        *decks.entry(&row.deck).or_default() += 1;
    }
    decks.into_iter().map(|(deck, n)| (deck.to_owned(), n)).collect()
}

/// The cards of `rows` to review by `until` in `deck` and the decks under it: those due, the
/// longest due first, then those never reviewed in their notes' order, `new` of them at most;
/// `limit` at most in all (as `CardIndex::due_cards`).
pub fn due_cards(rows: &[CardRow], deck: &str, until: i64, limit: usize, new: usize) -> Vec<DueCard> {
    let mut due: Vec<&CardRow> = rows.iter().filter(|row| under(row, deck) && !row.paused && row.reviewed.is_some_and(|s| s.due <= until)).collect();
    // Stable: the rows come by note, in the note's order.
    due.sort_by_key(|row| row.reviewed.map(|s| s.due));
    due.truncate(limit);
    let room = new.min(limit - due.len());
    let fresh = rows.iter().filter(|row| under(row, deck) && !row.paused && row.reviewed.is_none()).take(room);
    due.into_iter().chain(fresh).map(due_card).collect()
}

/// The reviewed cards of `rows` in `deck` and the decks under it not due by `after` yet, the
/// soonest due first, `limit` at most (as `CardIndex::cards_ahead`).
pub fn cards_ahead(rows: &[CardRow], deck: &str, after: i64, limit: usize) -> Vec<DueCard> {
    let mut ahead: Vec<&CardRow> = rows.iter().filter(|row| under(row, deck) && !row.paused && row.reviewed.is_some_and(|s| s.due > after)).collect();
    ahead.sort_by_key(|row| row.reviewed.map(|s| s.due));
    ahead.into_iter().take(limit).map(due_card).collect()
}
