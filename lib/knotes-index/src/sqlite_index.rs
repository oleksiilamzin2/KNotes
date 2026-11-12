use std::cell::{Cell, RefCell};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knotes_core::BoxError;
use rusqlite::Connection;

use crate::TextSnapshot;
use crate::content_pack::{ContentPack, PACK_FILE};
use crate::schema;

pub(crate) const DB_FILE: &str = "index.sqlite";

/// The index of one vault: `dir/index.sqlite` plus the note texts in `dir/content.pack`.
pub struct SqliteIndex {
    pub(crate) conn: Connection,
    pub(crate) pack: ContentPack,
    /// Cached number of search blocks; reset by every write.
    pub(crate) block_count: Cell<Option<i64>>,
    /// Cached text snapshot; reset by every write.
    pub(crate) corpus: RefCell<Option<Arc<TextSnapshot>>>,
}

impl SqliteIndex {
    /// Opens or creates the index. An index with another schema version, or one that does
    /// not match its content pack (e.g. after a crash), is deleted and created empty: it is
    /// a cache and gets rebuilt from the vault.
    pub fn open(dir: &Path) -> Result<Self, BoxError> {
        fs::create_dir_all(dir)?;
        if let Some(index) = Self::open_consistent(dir)? {
            return Ok(index);
        }
        for name in [DB_FILE, "index.sqlite-wal", "index.sqlite-shm", PACK_FILE] {
            match fs::remove_file(dir.join(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Self::open_consistent(dir)?.ok_or_else(|| "a fresh index is inconsistent".into())
    }

    /// Drops everything cached from the database; called after every write.
    pub(crate) fn invalidate_caches(&self) {
        self.block_count.set(None);
        self.corpus.borrow_mut().take();
    }

    fn open_consistent(dir: &Path) -> Result<Option<Self>, BoxError> {
        let conn = Connection::open(dir.join(DB_FILE))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION && version != 0 {
            return Ok(None);
        }
        conn.execute_batch(schema::PRAGMAS)?;
        // `rarray(?)` passes lists of paths (search scopes) as one parameter.
        rusqlite::vtab::array::load_module(&conn)?;
        if version == 0 {
            conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = {}; COMMIT;", schema::CREATE, schema::VERSION))?;
        }
        let pack = ContentPack::open(&dir.join(PACK_FILE))?;
        let needed: i64 = conn.query_row("SELECT COALESCE(MAX(pack_off + pack_len), 0) FROM files", [], |r| r.get(0))?;
        if needed as u64 > pack.len()? {
            return Ok(None);
        }
        Ok(Some(Self { conn, pack, block_count: Cell::new(None), corpus: RefCell::new(None) }))
    }
}

/// What one `SqliteIndex::maintenance` run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// FTS5 merge commands that did work.
    pub merge_steps: u32,
    /// WAL frames not yet copied into the database after the checkpoint (busy readers).
    pub wal_frames_left: i64,
}

/// Longest a single maintenance run keeps the writer busy.
const MAINTENANCE_BUDGET: Duration = Duration::from_millis(200);
/// Leaf pages merged per FTS5 `merge` command.
const MERGE_PAGES: i64 = 500;

impl SqliteIndex {
    /// Merges FTS5 segments until there is no work left or the time budget is spent, then
    /// checkpoints the WAL. Call repeatedly in idle time; each call is short.
    pub fn maintenance(&mut self) -> Result<MaintenanceReport, BoxError> {
        let mut report = MaintenanceReport::default();
        let start = Instant::now();
        while start.elapsed() < MAINTENANCE_BUDGET {
            let before = self.conn.total_changes();
            self.conn.execute("INSERT INTO blocks_fts(blocks_fts, rank) VALUES ('merge', ?1)", [MERGE_PAGES])?;
            // FTS5: fewer than two changes means the merge found nothing to do.
            if self.conn.total_changes() - before < 2 {
                break;
            }
            report.merge_steps += 1;
        }
        let (log, done): (i64, i64) = self.conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| Ok((r.get(1)?, r.get(2)?)))?;
        report.wal_frames_left = (log - done).max(0);
        self.invalidate_caches();
        Ok(report)
    }
}
