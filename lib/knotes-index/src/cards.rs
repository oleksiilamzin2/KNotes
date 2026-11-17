//! The vault's cards (ADR 0037): where each is written, its deck, and its schedule, worked out
//! from the reviews logged in the vault. The log is the record; these tables are rebuilt from
//! it with the rest of the index.

use std::collections::BTreeSet;

use knotes_cards::{CardKind, DeckSettings, Fsrs, Grade, parse_review, scan_cards, schedule};
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
            for (when, card, grade) in text[..whole].lines().filter_map(parse_review) {
                if insert.execute(params![card, when, grade as i64])? > 0 {
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
    let fsrs = Fsrs { retention: deck.map_or(fsrs.retention, |deck| settings.retention(&deck)), ..fsrs.clone() };
    if let Some(s) = schedule(&fsrs, reviews) {
        tx.prepare_cached("INSERT OR REPLACE INTO card_states(card_id, stability, difficulty, first, last, due, reviews, lapses) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
            .execute(params![id, s.memory.stability, s.memory.difficulty, s.first, s.last, s.due, s.reviews, s.lapses])?;
    }
    Ok(())
}

impl CardIndex for SqliteIndex {
    fn deck_counts(&self, until: i64) -> Result<Vec<DeckCount>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT c.deck, COUNT(*), COUNT(CASE WHEN s.due <= ?1 THEN 1 END), COUNT(CASE WHEN s.card_id IS NULL THEN 1 END)
             FROM cards c LEFT JOIN card_states s ON s.card_id = c.card_id GROUP BY c.deck ORDER BY c.deck",
        )?;
        let rows = st.query_map([until], |r| {
            Ok(DeckCount { deck: r.get(0)?, cards: r.get::<_, i64>(1)? as usize, due: r.get::<_, i64>(2)? as usize, fresh: r.get::<_, i64>(3)? as usize })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn due_cards(&self, deck: Option<&str>, until: i64, limit: usize, new: usize) -> Result<Vec<DueCard>, BoxError> {
        // A deck and those under it: `deck` itself, and the range [deck + '/', deck + '0').
        let (own, from, to) = match deck {
            Some(deck) => (deck.to_owned(), format!("{deck}/"), format!("{deck}0")),
            None => (String::new(), String::new(), "\u{10FFFF}".to_owned()),
        };
        let card = |r: &rusqlite::Row| Ok(DueCard { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, due: r.get(5)? });
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
             FROM cards c JOIN files f ON f.id = c.file_id JOIN card_states s ON s.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.due <= ?4
             ORDER BY s.due, f.path, c.ord LIMIT ?5",
        )?;
        let mut cards: Vec<DueCard> = st.query_map(params![own, from, to, until, limit as i64], card)?.collect::<Result<_, _>>()?;
        let new = new.min(limit - cards.len());
        if new > 0 {
            let mut st = self.conn.prepare_cached(
                "SELECT f.path, c.start, c.end, c.deck, c.card_id, NULL
                 FROM cards c JOIN files f ON f.id = c.file_id LEFT JOIN card_states s ON s.card_id = c.card_id
                 WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.card_id IS NULL
                 ORDER BY f.path, c.ord LIMIT ?4",
            )?;
            for row in st.query_map(params![own, from, to, new as i64], card)? {
                cards.push(row?);
            }
        }
        Ok(cards)
    }

    fn cards_ahead(&self, deck: &str, after: i64, limit: usize) -> Result<Vec<DueCard>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.due
             FROM cards c JOIN files f ON f.id = c.file_id JOIN card_states s ON s.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.due > ?4
             ORDER BY s.due, f.path, c.ord LIMIT ?5",
        )?;
        let rows = st.query_map(params![deck, format!("{deck}/"), format!("{deck}0"), after, limit as i64], |r| {
            Ok(DueCard { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, due: r.get(5)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn introduced(&self, since: i64) -> Result<Vec<(String, usize)>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT c.deck, COUNT(*) FROM card_states s JOIN cards c ON c.card_id = s.card_id WHERE s.first >= ?1 GROUP BY c.deck ORDER BY c.deck",
        )?;
        let rows = st.query_map([since], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn card_rows(&self, deck: Option<&str>, path: Option<&str>) -> Result<Vec<CardRow>, BoxError> {
        // As `due_cards` takes a deck and those under it.
        let (from, to) = (deck.map(|d| format!("{d}/")), deck.map(|d| format!("{d}0")));
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, c.start, c.end, c.deck, c.card_id, s.stability, s.difficulty, s.last, s.due, s.reviews, s.lapses
             FROM cards c JOIN files f ON f.id = c.file_id LEFT JOIN card_states s ON s.card_id = c.card_id
             WHERE (?1 IS NULL OR c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND (?4 IS NULL OR f.path = ?4)
             ORDER BY f.path, c.ord",
        )?;
        let rows = st.query_map(params![deck, from, to, path], |r| {
            let reviewed = match r.get::<_, Option<f64>>(5)? {
                Some(stability) => Some(Reviewed { state: CardState { stability, difficulty: r.get(6)?, last: r.get(7)? }, due: r.get(8)?, reviews: r.get(9)?, lapses: r.get(10)? }),
                None => None,
            };
            Ok(CardRow { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, deck: r.get(3)?, id: r.get(4)?, reviewed })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn deck_stats(&self, deck: &str, day_start: i64) -> Result<DeckStats, BoxError> {
        const DAY: i64 = 86_400;
        let (from, to) = (format!("{deck}/"), format!("{deck}0"));
        let mut stats = DeckStats { due_by_day: vec![0; FORECAST_DAYS + 1], reviews_by_day: vec![(0, 0); PAST_DAYS], ..DeckStats::default() };
        let count = |r: &rusqlite::Row, i: usize| r.get::<_, i64>(i).map(|n| n as usize);
        (stats.cards, stats.fresh, stats.mature) = self
            .conn
            .prepare_cached(
                "SELECT COUNT(*), COUNT(CASE WHEN s.card_id IS NULL THEN 1 END), COUNT(CASE WHEN s.due - s.last >= ?4 THEN 1 END)
                 FROM cards c LEFT JOIN card_states s ON s.card_id = c.card_id WHERE c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)",
            )?
            .query_row(params![deck, from, to, MATURE_DAYS * DAY], |r| Ok((count(r, 0)?, count(r, 1)?, count(r, 2)?)))?;
        stats.young = stats.cards - stats.fresh - stats.mature;
        let mut st = self.conn.prepare_cached(
            "SELECT CASE WHEN s.due < ?4 THEN 0 ELSE (s.due - ?4) / 86400 END, COUNT(*)
             FROM cards c JOIN card_states s ON s.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND s.due < ?5 GROUP BY 1",
        )?;
        for row in st.query_map(params![deck, from, to, day_start, day_start + (FORECAST_DAYS as i64 + 1) * DAY], |r| Ok((count(r, 0)?, count(r, 1)?)))? {
            let (day, cards) = row?;
            stats.due_by_day[day] = cards;
        }
        let since = day_start - (PAST_DAYS as i64 - 1) * DAY;
        let mut st = self.conn.prepare_cached(
            "SELECT (r.at - ?4) / 86400, COUNT(*), COUNT(CASE WHEN r.grade = 1 THEN 1 END)
             FROM cards c JOIN reviews r ON r.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND r.at >= ?4 AND r.at < ?5 GROUP BY 1",
        )?;
        for row in st.query_map(params![deck, from, to, since, day_start + DAY], |r| Ok((count(r, 0)?, count(r, 1)?, count(r, 2)?)))? {
            let (day, reviews, again) = row?;
            stats.reviews_by_day[day] = (reviews, again);
        }
        let mut st = self.conn.prepare_cached(
            "SELECT r.grade, COUNT(*) FROM cards c JOIN reviews r ON r.card_id = c.card_id
             WHERE (c.deck = ?1 OR (c.deck >= ?2 AND c.deck < ?3)) AND r.at >= ?4 AND r.at < ?5 GROUP BY 1",
        )?;
        for row in st.query_map(params![deck, from, to, since, day_start + DAY], |r| Ok((count(r, 0)?, count(r, 1)?)))? {
            let (grade, reviews) = row?;
            if let Some(of_grade) = stats.grades.get_mut(grade.wrapping_sub(1)) {
                *of_grade = reviews;
            }
        }
        Ok(stats)
    }

    fn card_state(&self, id: &str) -> Result<Option<CardState>, BoxError> {
        use rusqlite::OptionalExtension;
        let mut st = self.conn.prepare_cached("SELECT stability, difficulty, last FROM card_states WHERE card_id = ?1")?;
        Ok(st.query_row([id], |r| Ok(CardState { stability: r.get(0)?, difficulty: r.get(1)?, last: r.get(2)? })).optional()?)
    }

    fn deck_notes(&self, deck: &str) -> Result<Vec<String>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT DISTINCT f.path FROM cards c JOIN files f ON f.id = c.file_id
             WHERE c.deck = ?1 OR (c.deck >= ?1 || '/' AND c.deck < ?1 || '0') ORDER BY f.path",
        )?;
        let rows = st.query_map([deck], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
