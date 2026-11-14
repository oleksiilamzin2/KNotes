use std::collections::HashMap;
use std::path::Path;

use knotes_core::{BlockHit, BoxError, Span};
use knotes_embed::{DIMS, Vector};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::Snapshot;

/// The vectors of a vault's notes and PDFs, in `vectors.sqlite` beside its index (a file of its
/// own: the vectors are written in the background, never in the indexer's way). Each document's
/// row holds the date it had when embedded, its blocks' places and their vectors; a store made
/// with another model (or another way of embedding) is emptied, to be embedded again.
pub struct VectorStore {
    conn: Connection,
}

pub const VECTORS_FILE: &str = "vectors.sqlite";

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS notes(path TEXT PRIMARY KEY, mtime_ns INTEGER NOT NULL, places TEXT NOT NULL, vectors BLOB NOT NULL) WITHOUT ROWID;
";

impl VectorStore {
    /// The store in `dir` for writing vectors made by `model`.
    pub fn open(dir: &Path, model: &str) -> Result<Self, BoxError> {
        std::fs::create_dir_all(dir)?;
        let conn = Connection::open(dir.join(VECTORS_FILE))?;
        conn.execute_batch(SCHEMA)?;
        let kept: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'model'", [], |r| r.get(0)).optional()?;
        if kept.as_deref() != Some(model) {
            conn.execute_batch("DELETE FROM notes")?;
            conn.execute("INSERT OR REPLACE INTO meta VALUES ('model', ?1)", [model])?;
        }
        Ok(Self { conn })
    }

    /// The store in `dir` for searching (created empty if not there yet).
    pub fn open_reader(dir: &Path) -> Result<Self, BoxError> {
        std::fs::create_dir_all(dir)?;
        let conn = Connection::open(dir.join(VECTORS_FILE))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Each document embedded, with the date it had then.
    pub fn dates(&self) -> Result<HashMap<String, i64>, BoxError> {
        let mut st = self.conn.prepare_cached("SELECT path, mtime_ns FROM notes")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `blocks` of the document at `path`, dated `mtime_ns`, and their `vectors`, in place of
    /// what was kept for it.
    pub fn put(&self, path: &str, mtime_ns: i64, blocks: &[BlockHit], vectors: &[Vector]) -> Result<(), BoxError> {
        let places: Vec<Value> = blocks.iter().map(|b| json!([b.ord, b.span.start, b.span.end, b.heading])).collect();
        let bytes: Vec<u8> = vectors.iter().flatten().map(|b| *b as u8).collect();
        self.conn.prepare_cached("INSERT OR REPLACE INTO notes VALUES (?1, ?2, ?3, ?4)")?.execute(params![path, mtime_ns, Value::from(places).to_string(), bytes])?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> Result<(), BoxError> {
        self.conn.prepare_cached("DELETE FROM notes WHERE path = ?1")?.execute([path])?;
        Ok(())
    }

    /// Every block kept, in memory.
    pub fn snapshot(&self) -> Result<Snapshot, BoxError> {
        let mut snapshot = Snapshot::default();
        let mut st = self.conn.prepare_cached("SELECT path, places, vectors FROM notes")?;
        let mut rows = st.query([])?;
        while let Some(row) = rows.next()? {
            let (path, places, bytes): (String, String, Vec<u8>) = (row.get(0)?, row.get(1)?, row.get(2)?);
            let places: Value = serde_json::from_str(&places)?;
            for (place, vector) in places.as_array().into_iter().flatten().zip(bytes.as_chunks::<DIMS>().0) {
                let number = |i: usize| place[i].as_u64().unwrap_or(0) as u32;
                let block = BlockHit { path: path.clone(), ord: number(0), span: Span { start: number(1), end: number(2) }, heading: place[3].as_str().map(str::to_string), score: 0.0 };
                snapshot.push(block, vector.map(|b| b as i8));
            }
        }
        Ok(snapshot)
    }

    /// Changes whenever another connection has written (SQLite's `data_version`).
    pub fn version(&self) -> Result<i64, BoxError> {
        Ok(self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }
}
