use knotes_core::{BlockHit, BoxError, ContentQuery, ContentSearch, Span, Within};
use std::collections::HashSet;
use std::rc::Rc;

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params};

use crate::SqliteIndex;
use crate::fts_query::{QueryGroup, join, query_groups};
use crate::light_stemmer::LightStemmer;

/// A word in more than this share of blocks is "frequent": it adds almost nothing to bm25,
/// but ranking it means scoring nearly every block (23 ms for `і` at 50k notes on an M5).
const FREQUENT_SHARE: f64 = 0.2;
/// Below this many blocks ranking everything is cheap, so no word is dropped.
const MIN_BLOCKS_TO_DROP: i64 = 2000;

/// Top-k first (bm25, or any k matches in index order for queries made only of frequent
/// words), then joins only for the k winners. A scope keeps blocks of the listed paths.
fn sql(ranked: bool, scoped: bool) -> String {
    let (score, order) = if ranked { ("bm25(blocks_fts)", "ORDER BY score") } else { ("0.0", "") };
    let scope = if scoped {
        // Unary `+`: without it SQLite hands `rowid IN (...)` to FTS5 as an index constraint and
        // FTS5 re-runs the whole MATCH once per scoped rowid (8 s for a 10k-note scope).
        "AND +rowid IN (SELECT id FROM temp.scope_blocks)"
    } else {
        ""
    };
    format!(
        "WITH top AS (
             SELECT rowid, {score} AS score FROM blocks_fts
             WHERE blocks_fts MATCH ?1 {scope} {order} LIMIT ?2
         )
         SELECT f.path, b.ord, b.start, b.end, h.text, top.score
         FROM top
         JOIN blocks b ON b.id = top.rowid
         JOIN files f ON f.id = b.file_id
         LEFT JOIN headings h ON h.file_id = b.file_id AND h.ord = b.heading_ord
         ORDER BY top.score"
    )
}

impl ContentSearch for SqliteIndex {
    fn search(&self, query: &ContentQuery) -> Result<Vec<BlockHit>, BoxError> {
        self.follow_writer()?;
        let groups = query_groups(query.text, &LightStemmer, query.prefixes);
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(paths) = query.scope {
            self.load_scope(paths)?;
        }
        let scoped = query.scope.is_some();
        match query.within {
            Within::Block => self.ranked_blocks(&groups, scoped, query.limit),
            // One word: a note matches when one of its blocks does.
            Within::Note if groups.len() == 1 => Ok(first_per_note(self.ranked_blocks(&groups, scoped, query.limit * 4)?, query.limit)),
            Within::Note => self.notes_with_every_word(&groups, scoped, query.limit),
        }
    }
}

/// Blocks matching a note-level search are ranked among at most this many.
const NOTE_RANK_BLOCKS: i64 = 20_000;
/// More notes than this match a note-level search: they are shown unranked (bm25 over all
/// their blocks took ~37 ms for 7k notes at 50k on an M5).
const RANK_MAX_NOTES: usize = 1000;

impl SqliteIndex {
    /// Blocks with every word, best first; words in most blocks are left out of the ranking
    /// (and so of the requirement) when rarer ones are there.
    fn ranked_blocks(&self, groups: &[QueryGroup], scoped: bool, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        let blocks = self.blocks()?;
        let mut rare: Vec<&QueryGroup> = Vec::with_capacity(groups.len());
        if blocks >= MIN_BLOCKS_TO_DROP {
            for g in groups {
                if g.typed || !self.is_frequent(&g.word, blocks)? {
                    rare.push(g);
                }
            }
        } else {
            rare.extend(groups);
        }
        let ranked = !rare.is_empty();
        let expr = if ranked { join(rare) } else { join(groups) };
        let mut st = self.conn.prepare_cached(&sql(ranked, scoped))?;
        let rows = st.query_map(params![expr, limit as i64], block_hit)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Notes holding every word somewhere, each as its best block for all the words, best
    /// first. Each word's notes come from its own full-text match (FTS reads the word's whole
    /// posting list either way), intersected here, rarest word first so an empty
    /// intersection stops early. Thousands of notes are not ranked: the first `limit` are
    /// shown in index order, each with its first matching block.
    fn notes_with_every_word(&self, groups: &[QueryGroup], scoped: bool, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        let mut order = Vec::with_capacity(groups.len());
        for g in groups {
            // A short prefix goes last: its exact frequency says nothing about its matches.
            order.push(((g.typed, self.doc_count(&g.word)?), g));
        }
        order.sort_by_key(|(key, _)| *key);
        let restrict = if scoped { "AND +blocks_fts.rowid IN (SELECT id FROM temp.scope_blocks)" } else { "" };
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT DISTINCT b.file_id FROM blocks_fts JOIN blocks b ON b.id = blocks_fts.rowid WHERE blocks_fts MATCH ?1 {restrict}"
        ))?;
        let mut notes: Option<HashSet<i64>> = None;
        for (_, group) in order {
            let found: HashSet<i64> = st.query_map([&group.expr], |r| r.get(0))?.collect::<Result<_, _>>()?;
            let left: HashSet<i64> = match notes {
                None => found,
                Some(previous) => previous.intersection(&found).copied().collect(),
            };
            if left.is_empty() {
                return Ok(Vec::new());
            }
            notes = Some(left);
        }
        let mut notes: Vec<i64> = notes.unwrap_or_default().into_iter().collect();
        notes.sort_unstable();
        let ranked = notes.len() <= RANK_MAX_NOTES;
        if !ranked {
            notes.truncate(limit);
        }
        self.load_note_blocks(&notes, scoped)?;
        let any = groups.iter().map(|g| g.expr.as_str()).collect::<Vec<_>>().join(" OR ");
        let (score, order_by) = if ranked { ("bm25(blocks_fts)", "ORDER BY score") } else { ("0.0", "") };
        let mut st = self.conn.prepare_cached(&format!(
            "WITH top AS (
                 SELECT rowid, {score} AS score FROM blocks_fts
                 WHERE blocks_fts MATCH ?1 AND +rowid IN (SELECT id FROM temp.note_blocks) {order_by} LIMIT ?2
             )
             SELECT f.path, b.ord, b.start, b.end, h.text, top.score
             FROM top
             JOIN blocks b ON b.id = top.rowid
             JOIN files f ON f.id = b.file_id
             LEFT JOIN headings h ON h.file_id = b.file_id AND h.ord = b.heading_ord
             ORDER BY top.score"
        ))?;
        let hits = st.query_map(params![any, NOTE_RANK_BLOCKS], block_hit)?.collect::<Result<Vec<_>, _>>()?;
        Ok(first_per_note(hits, limit))
    }

    /// Fills `temp.note_blocks` with the blocks of `files` (within the scope, if any).
    fn load_note_blocks(&self, files: &[i64], scoped: bool) -> Result<(), BoxError> {
        let values: Rc<Vec<Value>> = Rc::new(files.iter().map(|&id| Value::Integer(id)).collect());
        self.conn.execute("DELETE FROM temp.note_blocks", [])?;
        let scope = if scoped { "AND b.id IN (SELECT id FROM temp.scope_blocks)" } else { "" };
        self.conn.execute(&format!("INSERT INTO temp.note_blocks(id) SELECT b.id FROM blocks b WHERE b.file_id IN rarray(?1) {scope}"), [values])?;
        Ok(())
    }

    /// Blocks holding `word` exactly (0 when it is not an indexed term).
    fn doc_count(&self, word: &str) -> Result<i64, BoxError> {
        let docs: Option<i64> =
            self.conn.prepare_cached("SELECT doc FROM blocks_vocab WHERE term = ?1")?.query_row([word], |r| r.get(0)).optional()?;
        Ok(docs.unwrap_or(0))
    }
}

/// The first (best) hit of each note, at most `limit` notes.
fn first_per_note(hits: Vec<BlockHit>, limit: usize) -> Vec<BlockHit> {
    let mut seen = HashSet::new();
    hits.into_iter().filter(|h| seen.insert(h.path.clone())).take(limit).collect()
}

fn block_hit(r: &rusqlite::Row) -> rusqlite::Result<BlockHit> {
    Ok(BlockHit {
        path: r.get(0)?,
        ord: r.get(1)?,
        span: Span { start: r.get(2)?, end: r.get(3)? },
        heading: r.get(4)?,
        score: r.get(5)?,
    })
}

impl SqliteIndex {
    /// Fills `temp.scope_blocks` with the blocks of `paths`, unless it already holds this scope:
    /// while a user types content queries the scope stays the same (10k-note scope: ~10 ms).
    fn load_scope(&self, paths: &[String]) -> Result<(), BoxError> {
        if self.scope.borrow().as_deref() == Some(paths) {
            return Ok(());
        }
        let values: Rc<Vec<Value>> = Rc::new(paths.iter().map(|p| Value::Text(p.clone())).collect());
        self.conn.execute("DELETE FROM temp.scope_blocks", [])?;
        self.conn.execute(
            "INSERT INTO temp.scope_blocks(id)
             SELECT b.id FROM files f JOIN blocks b ON b.file_id = f.id WHERE f.path IN rarray(?1)",
            [values],
        )?;
        *self.scope.borrow_mut() = Some(paths.to_vec());
        Ok(())
    }

    fn blocks(&self) -> Result<i64, BoxError> {
        if let Some(n) = self.block_count.get() {
            return Ok(n);
        }
        let n: i64 = self.conn.query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))?;
        self.block_count.set(Some(n));
        Ok(n)
    }

    fn is_frequent(&self, word: &str, blocks: i64) -> Result<bool, BoxError> {
        let docs: Option<i64> =
            self.conn.prepare_cached("SELECT doc FROM blocks_vocab WHERE term = ?1")?.query_row([word], |r| r.get(0)).optional()?;
        Ok(docs.unwrap_or(0) as f64 > blocks as f64 * FREQUENT_SHARE)
    }
}
