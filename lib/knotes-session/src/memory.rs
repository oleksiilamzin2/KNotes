//! Giving memory back after large index work (ADR 0006 memory budget, ADR 0014).

use knotes_index::SqliteIndex;

/// A sync touching at least this many files counts as large.
pub(crate) const LARGE_SYNC: usize = 256;

/// Frees SQLite's clean cache pages: a large write fills the writer's page cache, which it
/// otherwise keeps for the life of the connection.
pub(crate) fn give_back(index: &SqliteIndex) {
    index.release_memory();
}
