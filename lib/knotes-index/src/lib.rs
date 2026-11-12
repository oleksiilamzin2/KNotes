//! SQLite index of a vault (WAL mode, one writer, many readers).

mod codec;
mod read;
mod schema;
mod sqlite_index;
mod write;

pub use sqlite_index::SqliteIndex;
