use knotes_core::{BlockHit, BoxError, ContentSearch, Span};
use rusqlite::{OptionalExtension, params};

use crate::SqliteIndex;
use crate::fts_query::{QueryGroup, join, query_groups};
use crate::light_stemmer::LightStemmer;

/// A word in more than this share of blocks is "frequent": it adds almost nothing to bm25,
/// but ranking it means scoring nearly every block (23 ms for `і` at 50k notes on an M5).
const FREQUENT_SHARE: f64 = 0.2;
/// Below this many blocks ranking everything is cheap, so no word is dropped.
const MIN_BLOCKS_TO_DROP: i64 = 2000;

/// Top-k first, then joins only for the k winners.
const RANKED: &str = "
    WITH top AS (
        SELECT rowid, bm25(blocks_fts) AS score FROM blocks_fts
        WHERE blocks_fts MATCH ?1 ORDER BY score LIMIT ?2
    )
    SELECT f.path, b.ord, b.start, b.end, h.text, top.score
    FROM top
    JOIN blocks b ON b.id = top.rowid
    JOIN files f ON f.id = b.file_id
    LEFT JOIN headings h ON h.file_id = b.file_id AND h.ord = b.heading_ord
    ORDER BY top.score";

/// Any k matches in index order, for queries made only of frequent words.
const UNRANKED: &str = "
    WITH top AS (SELECT rowid FROM blocks_fts WHERE blocks_fts MATCH ?1 LIMIT ?2)
    SELECT f.path, b.ord, b.start, b.end, h.text, 0.0
    FROM top
    JOIN blocks b ON b.id = top.rowid
    JOIN files f ON f.id = b.file_id
    LEFT JOIN headings h ON h.file_id = b.file_id AND h.ord = b.heading_ord";

impl ContentSearch for SqliteIndex {
    fn search_blocks(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        let groups = query_groups(query, &LightStemmer);
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let blocks = self.blocks()?;
        let mut rare: Vec<&QueryGroup> = Vec::with_capacity(groups.len());
        if blocks >= MIN_BLOCKS_TO_DROP {
            for g in &groups {
                if !self.is_frequent(&g.word, blocks)? {
                    rare.push(g);
                }
            }
        } else {
            rare.extend(&groups);
        }
        let (sql, expr) = if rare.is_empty() { (UNRANKED, join(&groups)) } else { (RANKED, join(rare)) };
        let mut st = self.conn.prepare_cached(sql)?;
        let rows = st.query_map(params![expr, limit as i64], |r| {
            Ok(BlockHit {
                path: r.get(0)?,
                ord: r.get(1)?,
                span: Span { start: r.get(2)?, end: r.get(3)? },
                heading: r.get(4)?,
                score: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl SqliteIndex {
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
