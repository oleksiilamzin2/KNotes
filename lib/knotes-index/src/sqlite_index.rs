use std::fs;
use std::path::Path;

use knotes_core::BoxError;
use rusqlite::Connection;

use crate::schema;

pub(crate) const DB_FILE: &str = "index.sqlite";

/// The index of one vault, stored in `dir/index.sqlite`.
pub struct SqliteIndex {
    pub(crate) conn: Connection,
}

impl SqliteIndex {
    /// Opens or creates the index. An index with another schema version is deleted and recreated.
    pub fn open(dir: &Path) -> Result<Self, BoxError> {
        fs::create_dir_all(dir)?;
        let path = dir.join(DB_FILE);
        let mut conn = Connection::open(&path)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION && version != 0 {
            drop(conn);
            for suffix in ["", "-wal", "-shm"] {
                let _ = fs::remove_file(dir.join(format!("{DB_FILE}{suffix}")));
            }
            conn = Connection::open(&path)?;
        }
        conn.execute_batch(schema::PRAGMAS)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = {}; COMMIT;", schema::CREATE, schema::VERSION))?;
        }
        Ok(Self { conn })
    }
}
