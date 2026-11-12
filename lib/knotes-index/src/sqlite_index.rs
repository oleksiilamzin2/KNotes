use std::cell::Cell;
use std::fs;
use std::path::Path;

use knotes_core::BoxError;
use rusqlite::Connection;

use crate::content_pack::{ContentPack, PACK_FILE};
use crate::schema;

pub(crate) const DB_FILE: &str = "index.sqlite";

/// The index of one vault: `dir/index.sqlite` plus the note texts in `dir/content.pack`.
pub struct SqliteIndex {
    pub(crate) conn: Connection,
    pub(crate) pack: ContentPack,
    /// Cached number of search blocks; reset by every write.
    pub(crate) block_count: Cell<Option<i64>>,
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

    fn open_consistent(dir: &Path) -> Result<Option<Self>, BoxError> {
        let conn = Connection::open(dir.join(DB_FILE))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION && version != 0 {
            return Ok(None);
        }
        conn.execute_batch(schema::PRAGMAS)?;
        if version == 0 {
            conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = {}; COMMIT;", schema::CREATE, schema::VERSION))?;
        }
        let pack = ContentPack::open(&dir.join(PACK_FILE))?;
        let needed: i64 = conn.query_row("SELECT COALESCE(MAX(pack_off + pack_len), 0) FROM files", [], |r| r.get(0))?;
        if needed as u64 > pack.len()? {
            return Ok(None);
        }
        Ok(Some(Self { conn, pack, block_count: Cell::new(None) }))
    }
}
