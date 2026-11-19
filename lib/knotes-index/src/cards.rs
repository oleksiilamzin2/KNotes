//! The vault's cards (ADR 0037): where each is written, its deck, and its schedule, worked out
//! from the reviews logged in the vault. The log is the record; these tables are rebuilt from
//! it with the rest of the index.

use std::collections::BTreeSet;

use knotes_cards::{CardKind, DeckSettings, Fsrs, Grade, Logged, parse_logged, scan_cards, schedule};
use knotes_core::{BoxError, CardIndex, CardRow, CardState, DeckCount, DeckStats, DueCard, FORECAST_DAYS, MATURE_DAYS, PAST_DAYS, Reviewed};
use rusqlite::{Transaction, params};

use crate::SqliteIndex;

/// The cards of a note's text, written as its rows.
pub(crate) fn insert_cards(tx: &Transaction, file_id: i64, source: &str) -> Result<(), BoxError> {
    // Most notes hold none: not scanned line by line without the mark in them.
    if !source.contains(knotes_cards::DECK_MARK) {
        return Ok(());
    }
    let mut st = tx.prepare_cached("INSERT INTO cards(file_id, ord, deck, card_id, kind, start, end) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
    for (ord, card) in scan_cards(source).iter().enumerate() {
        let kind = match card.kind {
            CardKind::Basic => 0,
            CardKind::Cloze => 1,
        };
        st.execute(params![file_id, ord as i64, card.deck, card.id, kind, card.span.start as i64, card.span.end as i64])?;
    }
    Ok(())
}

/// The decks under `deck`, as a range of their paths: from `deck/` up to, not with, `deck0`
/// (`0` is the byte after `/`). The deck `""` is every deck: all of them are under it.
fn under(deck: &str) -> (String, String) {
    if deck.is_empty() { (String::new(), "\u{10FFFF}".to_owned()) } else { (format!("{deck}/"), format!("{deck}0")) }
}

impl SqliteIndex {
    /// Takes `text` (what `.knotes/decks.json` holds) as the decks' settings: when it is not
    /// the one the schedule was worked out with, every reviewed card's is worked out again
    /// (a deck's chance of recall moves when its cards are due). Says whether it was.
    pub fn take_deck_settings(&mut self, text: &str, fsrs: &Fsrs) -> Result<bool, BoxError> {
        use rusqlite::OptionalExtension;
        let tx = self.conn.transaction()?;
        let had: Option<String> = tx.query_row("SELECT text FROM deck_settings", [], |r| r.get(0)).optional()?;
        // An index new, or built again: its reviews are taken with these settings at once.
        if had.as_deref() == Some(text) {
            return Ok(false);
        }
        let settings = DeckSettings::parse(text);
        let ids: Vec<String> = tx.prepare("SELECT card_id FROM card_states")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for id in &ids {
            put_schedule(&tx, id, fsrs, &settings)?;
        }
        tx.execute("INSERT OR REPLACE INTO deck_settings(id, text) VALUES (0, ?1)", [text])?;
        tx.commit()?;
        Ok(had.is_some() || !ids.is_empty())
    }

    /// How much of the review log `name` has been read (bytes).
    pub fn reviews_read(&self, name: &str) -> Result<u64, BoxError> {
        let mut st = self.conn.prepare_cached("SELECT read FROM review_logs WHERE name = ?1")?;
        let read: Option<i64> = st.query_row([name], |r| r.get(0)).ok();
        Ok(read.unwrap_or(0) as u64)
    }

    /// Takes in `text`, what the review log `name` holds from byte `from` on (as `reviews_read`
    /// gave it): its whole lines' reviews are kept, and the schedule of each card they are of is
    /// worked out again from all its reviews, in time's order, whatever log they came from (a
    /// second Mac's arrive late), due at the chance of recall its deck sets (`settings`; a card
    /// no note holds, at `fsrs`'s). Gives how many reviews were new.
    pub fn take_reviews(&mut self, name: &str, from: u64, text: &str, fsrs: &Fsrs, settings: &DeckSettings) -> Result<usize, BoxError> {
        // A line still being written waits for its end.
        let whole = text.rfind('\n').map_or(0, |end| end + 1);
        let tx = self.conn.transaction()?;
        let mut cards: BTreeSet<&str> = BTreeSet::new();
        let mut new = 0;
        {
            let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO reviews(card_id, at, grade) VALUES (?1, ?2, ?3)")?;
            let mut take_back = tx.prepare_cached("DELETE FROM reviews WHERE card_id = ?1 AND at = ?2")?;
            // The latest by its time, whatever log it came from and when (a second Mac's
            // arrive late): an older line does not undo a newer one. Of two in one second, the
            // one that resumes stands, in whatever order the logs are read: a card is not left
            // out of sight by chance.
            let mut pause = tx.prepare_cached(
                "INSERT INTO card_pauses(card_id, at, paused) VALUES (?1, ?2, ?3)
                 ON CONFLICT(card_id) DO UPDATE SET at = excluded.at, paused = excluded.paused
                 WHERE excluded.at > card_pauses.at OR (excluded.at = card_pauses.at AND excluded.paused < card_pauses.paused)",
            )?;
            // In the log's order: a review taken back is taken back by a line after it.
            for logged in text[..whole].lines().filter_map(parse_logged) {
                let (changed, card) = match logged {
                    Logged::Review(when, card, grade) => (insert.execute(params![card, when, grade as i64])?, card),
                    Logged::TakenBack(when, card) => (take_back.execute(params![card, when])?, card),
                    // Not a review: nothing of the card's schedule changes.
                    Logged::Paused(when, card, paused) => {
                        new += pause.execute(params![card, when, paused])?;
                        continue;
                    }
                };
                if changed > 0 {
                    new += 1;
                    cards.insert(card);
                }
            }
            for card in cards {
                put_schedule(&tx, card, fsrs, settings)?;
            }
            tx.prepare_cached("INSERT OR REPLACE INTO review_logs(name, read) VALUES (?1, ?2)")?.execute(params![name, (from + whole as u64) as i64])?;
        }
        tx.commit()?;
        Ok(new)
    }
}

/// Works out the schedule of the card `id` again from all its reviews, in time's order.
fn put_schedule(tx: &Transaction, id: &str, fsrs: &Fsrs, settings: &DeckSettings) -> Result<(), BoxError> {
    use rusqlite::OptionalExtension;
    let reviews: Vec<(i64, Grade)> = tx
        .prepare_cached("SELECT at, grade FROM reviews WHERE card_id = ?1 ORDER BY at")?
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .filter_map(|row| row.ok().and_then(|(at, grade)| Some((at, Grade::from_number(grade)?))))
        .collect();
    let deck: Option<String> = tx.prepare_cached("SELECT deck FROM cards WHERE card_id = ?1 LIMIT 1")?.query_row([id], |r| r.get(0)).optional()?;
    // Its deck's chance of recall and steps; a card no note holds, the defaults.
    let steps = deck.as_deref().map_or_else(knotes_cards::Steps::default, |deck| settings.steps(deck));
    let fsrs = Fsrs { retention: deck.map_or(fsrs.retention, |deck| settings.retention(&deck)), ..fsrs.clone() };
    match schedule(&fsrs, &steps, reviews) {
        Some(s) => {
            tx.prepare_cached(
                "INSERT OR REPLACE INTO card_states(card_id, stability, difficulty, first, last, due, reviews, lapses, step, relearning) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?
            .execute(params![id, s.memory.stability, s.memory.difficulty, s.first, s.last, s.due, s.reviews, s.lapses, s.step, s.relearning])?;
        }
        // Its only review taken back: never reviewed again.
        None => {
            tx.prepare_cached("DELETE FROM card_states WHERE card_id = ?1")?.execute([id])?;
        }
    }
    Ok(())
}

impl CardIndex for SqliteIndex {
    fn deck_counts(&self, until: i64) -> Result<Vec<DeckCount>, BoxError> {
        let mut st = self.conn.prepare_cached(
            // A card paused is the deck's, but neither due nor new: it does not wait.
            "SELECT c.deck, COUNT(*), COUNT(CASE WHEN s.due <= ?1 AND COALESCE(p.paused, 0) = 0 THEN 1 END), COUNT(CASE WHEN s.card_id IS NULL AND COALESCE(p.paused, 0) = 0 THEN 1 END),
                    COUNT(CASE WHEN s.due <= ?1 AND s.step IS NOT NULL AND COALESCE(p.paused, 0) = 0 THEN 1 END)
             FROM cards c LEFT JOIN card_states s ON s.card_id = c.card_id LEFT JOIN card_pauses p ON p.card_id = c.card_id GROUP BY c.deck ORDER BY c.deck",
        )?;
        let rows = st.query_map([until], |r| {
            Ok(DeckCount { deck: r.get(0)?, cards: r.get::<_, i64>(1)? as usize, due: r.get::<_, i64>(2)? as usize, fresh: r.get::<_, i64>(3)? as usize, learning: r.get::<_, i64>(4)? as usize })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn reviewed(&self, since: i64) -> Result<Vec<(String, usize)>, BoxError> {
        let mut st = self.conn.prepare_cached(
            // The day's reviews first, then their cards, as `introduced` reads them.
            "SELECT c.deck, COUNT(*) FROM card_states s CROSS JOIN cards c ON c.card_id = s.card_id WHERE s.last >= ?1 AND s.first < ?1 AND s.step IS NULL GROUP BY c.deck ORDER BY c.deck",
        )?;
        let rows = st.query_map([since], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn due_cards(&self, deck: Option<&str>, until: i64, limit: usize, new: usize, reviews: usize) -> Result<Vec<DueCard>, BoxError> {
        // A deck and those under it: `deck` itself, and the range [deck + '/', deck + '0').
        let (own, (from, to)) = (deck.unwrap_or_default().to_owned(), under(deck.unwrap_or_default()));
        let card = |r: &rusqlite::Row| Ok(DueCard { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, due: r.get(5)? });
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
             FROM cards c JOIN files f ON f.id = c.file_id JOIN card_states s ON s.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.due <= ?4 AND (s.step IS NOT NULL) = ?6 AND NOT EXISTS (SELECT 1 FROM card_pauses p WHERE p.card_id = c.card_id AND p.paused = 1)
             ORDER BY s.due, f.path, c.ord LIMIT ?5",
        )?;
        // Every deck (none asked for, or `""`): the schedules in the order they are due, read as
        // far as the limit (CROSS JOIN keeps that order); by deck, SQLite takes every due card
        // of the vault to sort them.
        let mut every = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
             FROM card_states s CROSS JOIN cards c ON c.card_id = s.card_id JOIN files f ON f.id = c.file_id
             WHERE s.due <= ?1 AND (s.step IS NOT NULL) = ?3 AND NOT EXISTS (SELECT 1 FROM card_pauses p WHERE p.card_id = c.card_id AND p.paused = 1)
             ORDER BY s.due, f.path, c.ord LIMIT ?2",
        )?;
        // Those still at a step, all; those whose days have begun, what the day still takes.
        let mut cards: Vec<DueCard> = Vec::new();
        for (most, at_step) in [(limit, true), (reviews.min(limit), false)] {
            let found = if own.is_empty() { every.query_map(params![until, most as i64, at_step], card)? } else { st.query_map(params![own, from, to, until, most as i64, at_step], card)? };
            for row in found {
                cards.push(row?);
            }
        }
        // The longest due first, whichever they are.
        cards.sort_by_key(|card| card.due);
        cards.truncate(limit);
        let new = new.min(limit - cards.len());
        if new > 0 {
            let mut st = self.conn.prepare_cached(
                "SELECT f.path, c.start, c.end, c.deck, c.card_id, NULL
                 FROM cards c JOIN files f ON f.id = c.file_id LEFT JOIN card_states s ON s.card_id = c.card_id
                 WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.card_id IS NULL AND NOT EXISTS (SELECT 1 FROM card_pauses p WHERE p.card_id = c.card_id AND p.paused = 1)
                 ORDER BY f.path, c.ord LIMIT ?4",
            )?;
            for row in st.query_map(params![own, from, to, new as i64], card)? {
                cards.push(row?);
            }
        }
        Ok(cards)
    }

    fn cards_ahead(&self, deck: &str, after: i64, limit: usize) -> Result<Vec<DueCard>, BoxError> {
        let card = |r: &rusqlite::Row| Ok(DueCard { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, due: r.get(5)? });
        // Every deck (`""`): the schedules in the order they are due, read as far as the limit
        // (CROSS JOIN keeps that order). By deck, SQLite took every reviewed card of the vault
        // to sort them: 29 ms for 65 000.
        if deck.is_empty() {
            let mut st = self.conn.prepare_cached(
                "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
                 FROM card_states s CROSS JOIN cards c ON c.card_id = s.card_id JOIN files f ON f.id = c.file_id
                 WHERE s.due > ?1 AND NOT EXISTS (SELECT 1 FROM card_pauses p WHERE p.card_id = c.card_id AND p.paused = 1)
                 ORDER BY s.due, f.path, c.ord LIMIT ?2",
            )?;
            return Ok(st.query_map(params![after, limit as i64], card)?.collect::<Result<_, _>>()?);
        }
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
             FROM cards c JOIN files f ON f.id = c.file_id JOIN card_states s ON s.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.due > ?4 AND NOT EXISTS (SELECT 1 FROM card_pauses p WHERE p.card_id = c.card_id AND p.paused = 1)
             ORDER BY s.due, f.path, c.ord LIMIT ?5",
        )?;
        let (from, to) = under(deck);
        let rows = st.query_map(params![deck, from, to, after, limit as i64], |r| {
            Ok(DueCard { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, due: r.get(5)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn introduced(&self, since: i64) -> Result<Vec<(String, usize)>, BoxError> {
        let mut st = self.conn.prepare_cached(
            // The day's few first reviews, then their cards (CROSS JOIN keeps that order: SQLite
            // read every card to have them by deck).
            "SELECT c.deck, COUNT(*) FROM card_states s CROSS JOIN cards c ON c.card_id = s.card_id WHERE s.first >= ?1 GROUP BY c.deck ORDER BY c.deck",
        )?;
        let rows = st.query_map([since], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn card_rows(&self, deck: Option<&str>, path: Option<&str>) -> Result<Vec<CardRow>, BoxError> {
        // As `due_cards` takes a deck and those under it.
        let (from, to) = deck.map(under).unzip();
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.stability, s.difficulty, s.last, s.due, s.reviews, s.lapses, s.first, s.step, s.relearning,
                    COALESCE((SELECT p.paused FROM card_pauses p WHERE p.card_id = c.card_id), 0)
             FROM cards c JOIN files f ON f.id = c.file_id LEFT JOIN card_states s ON s.card_id = c.card_id
             WHERE (?1 IS NULL OR c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND (?4 IS NULL OR f.path = ?4)
             ORDER BY f.path, c.ord",
        )?;
        let rows = st.query_map(params![deck, from, to, path], |r| {
            let reviewed = match r.get::<_, Option<f64>>(5)? {
                Some(stability) => Some(Reviewed { state: CardState { stability, difficulty: r.get(6)?, last: r.get(7)?, step: r.get(12)?, relearning: r.get(13)? }, first: r.get(11)?, due: r.get(8)?, reviews: r.get(9)?, lapses: r.get(10)? }),
                None => None,
            };
            Ok(CardRow { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, reviewed, paused: r.get(14)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn deck_stats(&self, deck: &str, day_start: i64, holds: &dyn Fn(&str) -> bool) -> Result<DeckStats, BoxError> {
        const DAY: i64 = 86_400;
        let mut stats = DeckStats { due_by_day: vec![0; FORECAST_DAYS + 1], reviews_by_day: vec![(0, 0); PAST_DAYS], ..DeckStats::default() };
        // Counted by SQLite a note at a time, not read a card at a time: which notes count is
        // the caller's to say (a space's), so a note's counts come with its path, and a
        // hundred thousand cards are a thousand rows here. A card never reviewed has the day -1;
        // one due before today, today's.
        let (from, to) = under(deck);
        // Every deck (`""`): the cards as they lie, with no deck looked up for each (asked by
        // deck, a hundred thousand cards took twice as long). The three are named all the same,
        // so one list of values serves both.
        let of_deck = if deck.is_empty() { "(?1 || ?2 || ?3) IS NOT NULL" } else { "(c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3))" };
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT f.path, CASE WHEN s.card_id IS NULL THEN -1 WHEN s.due < ?4 THEN 0 ELSE (s.due - ?4) / 86400 END AS day, COUNT(*),
                    COUNT(CASE WHEN s.due - s.last >= ?5 THEN 1 END), COALESCE(p.paused, 0) AS paused
             FROM cards c JOIN files f ON f.id = c.file_id LEFT JOIN card_states s ON s.card_id = c.card_id LEFT JOIN card_pauses p ON p.card_id = c.card_id
             WHERE {of_deck} GROUP BY c.file_id, day, paused"
        ))?;
        let rows = st.query_map(params![deck, from, to, day_start, MATURE_DAYS * DAY], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)? as usize, r.get::<_, i64>(3)? as usize, r.get::<_, bool>(4)?)))?;
        for row in rows {
            let (path, day, cards, mature, paused) = row?;
            if !holds(&path) {
                continue;
            }
            stats.cards += cards;
            // Paused, a card is the deck's still, but of no kind and not coming due.
            if paused {
                stats.paused += cards;
                continue;
            }
            if day < 0 {
                stats.fresh += cards;
                continue;
            }
            stats.mature += mature;
            stats.young += cards - mature;
            if let Some(due) = stats.due_by_day.get_mut(day as usize) {
                *due += cards;
            }
        }
        // The reviews of the last days, counted the same way: by note, day and grade.
        let since = day_start - (PAST_DAYS as i64 - 1) * DAY;
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT f.path, (r.at - ?4) / 86400, r.grade, COUNT(*) FROM cards c JOIN files f ON f.id = c.file_id JOIN reviews r ON r.card_id = c.card_id
             WHERE {of_deck} AND r.at >= ?4 AND r.at < ?5 GROUP BY c.file_id, 2, r.grade"
        ))?;
        let rows = st.query_map(params![deck, from, to, since, day_start + DAY], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)? as usize)))?;
        for row in rows {
            let (path, day, grade, reviews) = row?;
            if !holds(&path) {
                continue;
            }
            if let Some(of_day) = stats.reviews_by_day.get_mut(day as usize) {
                of_day.0 += reviews;
                of_day.1 += if grade == 1 { reviews } else { 0 };
            }
            if let Some(of_grade) = stats.grades.get_mut((grade as usize).wrapping_sub(1)) {
                *of_grade += reviews;
            }
        }
        Ok(stats)
    }

    fn card_state(&self, id: &str) -> Result<Option<CardState>, BoxError> {
        use rusqlite::OptionalExtension;
        let mut st = self.conn.prepare_cached("SELECT stability, difficulty, last, step, relearning FROM card_states WHERE card_id = ?1")?;
        Ok(st.query_row([id], |r| Ok(CardState { stability: r.get(0)?, difficulty: r.get(1)?, last: r.get(2)?, step: r.get(3)?, relearning: r.get(4)? })).optional()?)
    }

    fn deck_notes(&self, deck: &str) -> Result<Vec<String>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT DISTINCT f.path FROM cards c JOIN files f ON f.id = c.file_id
             WHERE c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3) ORDER BY f.path",
        )?;
        let (from, to) = under(deck);
        let rows = st.query_map(params![deck, from, to], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
