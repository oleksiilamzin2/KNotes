use knotes_core::{BlockHit, BoxError, ContentSearch, Span};
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
        "AND +rowid IN (SELECT b.id FROM files f JOIN blocks b ON b.file_id = f.id WHERE f.path IN rarray(?3))"
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
    fn search_blocks(&self, query: &str, scope: Option<&[String]>, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
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
        let ranked = !rare.is_empty();
        let expr = if ranked { join(rare) } else { join(&groups) };
        let mut st = self.conn.prepare_cached(&sql(ranked, scope.is_some()))?;
        let rows = match scope {
            Some(paths) => {
                let paths: Rc<Vec<Value>> = Rc::new(paths.iter().map(|p| Value::Text(p.clone())).collect());
                st.query_map(params![expr, limit as i64, paths], block_hit)?.collect::<Result<_, _>>()
            }
            None => st.query_map(params![expr, limit as i64], block_hit)?.collect::<Result<_, _>>(),
        };
        Ok(rows?)
    }
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
