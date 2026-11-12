//! Schema of the index. The index is a cache: on a version change it is rebuilt, not migrated.

pub(crate) const VERSION: i64 = 2;

pub(crate) const CREATE: &str = "
CREATE TABLE files(
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    kind INTEGER NOT NULL,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    -- Range of the note text in content.pack; NULL for files that are not notes.
    pack_off INTEGER,
    pack_len INTEGER
);
CREATE TABLE headings(
    file_id INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    level INTEGER NOT NULL,
    text TEXT NOT NULL,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL,
    PRIMARY KEY(file_id, ord)
) WITHOUT ROWID;
CREATE TABLE links(
    file_id INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    embed INTEGER NOT NULL,
    target TEXT NOT NULL,
    subpath TEXT,
    display TEXT,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL,
    PRIMARY KEY(file_id, ord)
) WITHOUT ROWID;
CREATE TABLE tags(
    file_id INTEGER NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY(file_id, tag)
) WITHOUT ROWID;
CREATE INDEX tags_by_tag ON tags(tag, file_id);
";

/// Set on every connection.
pub(crate) const PRAGMAS: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
PRAGMA mmap_size = 268435456;
";
