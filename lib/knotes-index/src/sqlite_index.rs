use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use knotes_core::BoxError;
use rusqlite::{Connection, OpenFlags};

use crate::TextSnapshot;
use crate::content_pack::{ContentPack, HEADER_LEN, PACK_FILE};
use crate::schema;

pub(crate) const DB_FILE: &str = "index.sqlite";

/// The index of one vault: `dir/index.sqlite` plus the note texts in `dir/content.pack`.
pub struct SqliteIndex {
    dir: PathBuf,
    pub(crate) conn: Connection,
    /// In a cell: a reader reopens it when the writer compacts the pack.
    pub(crate) pack: RefCell<ContentPack>,
    /// Opened with `open_reader`: read-only, follows another connection's writes.
    read_only: bool,
    /// Last seen `PRAGMA data_version`; it changes when another connection commits.
    data_version: Cell<i64>,
    /// Cached number of search blocks; reset by every write.
    pub(crate) block_count: Cell<Option<i64>>,
    /// Cached text snapshot; reset by every write.
    pub(crate) corpus: RefCell<Option<Arc<TextSnapshot>>>,
    /// Paths whose blocks are in `temp.scope_blocks`; reset by every write.
    pub(crate) scope: RefCell<Option<Vec<String>>>,
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
        for name in [DB_FILE, "index.sqlite-wal", "index.sqlite-shm", PACK_FILE, "content.pack.new"] {
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
        self.scope.borrow_mut().take();
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
        let pack = match ContentPack::open(&dir.join(PACK_FILE)) {
            Ok(pack) => pack,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let generation: i64 = conn.query_row("SELECT value FROM meta WHERE key = 'pack_generation'", [], |r| r.get(0))?;
        if generation as u64 != pack.generation() {
            return Ok(None);
        }
        let needed: i64 = conn.query_row("SELECT COALESCE(MAX(pack_off + pack_len), 0) FROM files", [], |r| r.get(0))?;
        if needed as u64 > pack.len()? {
            return Ok(None);
        }
        conn.execute_batch("CREATE TEMP TABLE scope_blocks(id INTEGER PRIMARY KEY); CREATE TEMP TABLE note_blocks(id INTEGER PRIMARY KEY)")?;
        Ok(Some(Self { dir: dir.to_path_buf(), conn, pack: RefCell::new(pack), read_only: false, data_version: Cell::new(-1), block_count: Cell::new(None), corpus: RefCell::new(None), scope: RefCell::new(None) }))
    }
}

/// What one `SqliteIndex::maintenance` run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// FTS5 merge commands that did work.
    pub merge_steps: u32,
    /// WAL frames not yet copied into the database after the checkpoint (busy readers).
    pub wal_frames_left: i64,
    /// The content pack was rewritten without dead versions.
    pub compacted: bool,
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
        let dead = self.dead_pack_bytes()?;
        if dead >= COMPACT_MIN_DEAD && dead > self.pack.borrow().len()? - dead {
            report.compacted = self.compact_pack()?;
        }
        let (log, done): (i64, i64) = self.conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| Ok((r.get(1)?, r.get(2)?)))?;
        report.wal_frames_left = (log - done).max(0);
        self.invalidate_caches();
        Ok(report)
    }
}

impl SqliteIndex {
    /// Frees the clean pages of the writer's page cache. A large write (a cold build) fills
    /// the cache, which SQLite otherwise keeps for the life of the connection.
    pub fn release_memory(&self) {
        if let Err(e) = self.conn.release_memory() {
            eprintln!("knotes: sqlite release_memory: {e}");
        }
    }

    /// Bytes held by the writer's page cache.
    pub fn cache_used(&self) -> usize {
        let (mut current, mut high) = (0, 0);
        // SAFETY: a valid connection handle and out-pointers; reading a status counter.
        unsafe {
            rusqlite::ffi::sqlite3_db_status(self.conn.handle(), rusqlite::ffi::SQLITE_DBSTATUS_CACHE_USED, &mut current, &mut high, 0);
        }
        current.max(0) as usize
    }
}

/// Maintenance compacts the pack when at least this many bytes are dead and dead bytes
/// outnumber live ones.
const COMPACT_MIN_DEAD: u64 = 16 << 20;

impl SqliteIndex {
    /// Rewrites the content pack without dead versions. Returns `false` if nothing was dead.
    /// The new file is renamed over the old one inside the transaction that moves the offsets,
    /// so any crash leaves pack and index with different generations, which `open` detects.
    /// Snapshots taken before keep reading the old file.
    pub fn compact_pack(&mut self) -> Result<bool, BoxError> {
        let rows: Vec<(i64, u64, u64)> = self
            .conn
            .prepare_cached("SELECT id, pack_off, pack_len FROM files WHERE pack_off IS NOT NULL ORDER BY pack_off")?
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64)))?
            .collect::<Result<_, _>>()?;
        let live: u64 = rows.iter().map(|r| r.2).sum();
        if self.pack.borrow().len()? == HEADER_LEN + live {
            return Ok(false);
        }
        let generation = self.pack.borrow().generation() + 1;
        let (path, tmp) = (self.dir.join(PACK_FILE), self.dir.join(format!("{PACK_FILE}.new")));
        let ranges: Vec<(u64, u64)> = rows.iter().map(|r| (r.1, r.2)).collect();
        let offsets = self.pack.borrow().write_compacted(&tmp, generation, &ranges)?;
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare_cached("UPDATE files SET pack_off = ?2 WHERE id = ?1")?;
            for (row, off) in rows.iter().zip(&offsets) {
                st.execute([row.0, *off as i64])?;
            }
            tx.execute("UPDATE meta SET value = ?1 WHERE key = 'pack_generation'", [generation as i64])?;
        }
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        tx.commit()?;
        *self.pack.borrow_mut() = ContentPack::open(&path)?;
        self.invalidate_caches();
        Ok(true)
    }

    /// Dead bytes in the pack.
    fn dead_pack_bytes(&self) -> Result<u64, BoxError> {
        let live: i64 = self.conn.query_row("SELECT COALESCE(SUM(pack_len), 0) FROM files", [], |r| r.get(0))?;
        Ok(self.pack.borrow().len()?.saturating_sub(HEADER_LEN + live as u64))
    }
}

impl SqliteIndex {
    /// Opens an existing index read-only, next to a writer in another connection or process
    /// (WAL allows both). Never creates, migrates or deletes anything: an index of another
    /// schema version is an error.
    pub fn open_reader(dir: &Path) -> Result<Self, BoxError> {
        let path = dir.join(DB_FILE);
        if !path.exists() {
            return Err(format!("no index in {}", dir.display()).into());
        }
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA mmap_size = 268435456; PRAGMA cache_size = -16384;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION {
            return Err(format!("index schema {version}, expected {}", schema::VERSION).into());
        }
        rusqlite::vtab::array::load_module(&conn)?;
        conn.execute_batch("CREATE TEMP TABLE scope_blocks(id INTEGER PRIMARY KEY); CREATE TEMP TABLE note_blocks(id INTEGER PRIMARY KEY)")?;
        let pack = ContentPack::open_read_only(&dir.join(PACK_FILE))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            conn,
            pack: RefCell::new(pack),
            read_only: true,
            data_version: Cell::new(-1),
            block_count: Cell::new(None),
            corpus: RefCell::new(None),
            scope: RefCell::new(None),
        })
    }

    /// For readers: drops caches when another connection has committed since the last call.
    pub(crate) fn follow_writer(&self) -> Result<(), BoxError> {
        if !self.read_only {
            return Ok(());
        }
        let version: i64 = self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        if version != self.data_version.replace(version) {
            self.invalidate_caches();
        }
        Ok(())
    }

    /// Makes sure the pack matches `generation` (read in the same query as the offsets that
    /// will be used); a reader reopens it after the writer compacted.
    pub(crate) fn ensure_pack_generation(&self, generation: i64) -> Result<(), BoxError> {
        if generation as u64 != self.pack.borrow().generation() {
            *self.pack.borrow_mut() = ContentPack::open_read_only(&self.dir.join(PACK_FILE))?;
            if generation as u64 != self.pack.borrow().generation() {
                return Err("content pack changed while reading; retry".into());
            }
        }
        Ok(())
    }
}
