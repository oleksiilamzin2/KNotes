//! SQLite index of a vault (WAL mode, one writer, many readers).

mod codec;
mod content_pack;
mod fts_query;
mod read;
mod schema;
mod sqlite_index;
mod write;

pub use sqlite_index::SqliteIndex;
