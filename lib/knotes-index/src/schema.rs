//! Schema of the index. The index is a cache: on a version change it is rebuilt, not migrated.

pub(crate) const VERSION: i64 = 9;

pub(crate) const CREATE: &str = "
CREATE TABLE meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL) WITHOUT ROWID;
-- Must equal the generation in the content.pack header.
INSERT INTO meta VALUES ('pack_generation', 0);
CREATE TABLE files(
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    kind INTEGER NOT NULL,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    -- Folded file name used to resolve links (name_key.rs).
    name_key TEXT NOT NULL,
    -- Range of the note text in content.pack; NULL for files that are not notes.
    pack_off INTEGER,
    pack_len INTEGER,
    -- Note-list preview; NULL for files that are not notes.
    snippet TEXT
);
CREATE INDEX files_by_name ON files(name_key);
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
    -- Folded target name; NULL for external links and links inside the same note.
    target_key TEXT,
    -- Resolved file; NULL when broken or external.
    target_id INTEGER,
    PRIMARY KEY(file_id, ord)
) WITHOUT ROWID;
CREATE INDEX links_by_key ON links(target_key);
CREATE INDEX links_by_target ON links(target_id);
CREATE TABLE tags(
    file_id INTEGER NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY(file_id, tag)
) WITHOUT ROWID;
CREATE INDEX tags_by_tag ON tags(tag, file_id);
CREATE TABLE aliases(
    file_id INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY(file_id, ord)
) WITHOUT ROWID;
-- Search blocks; `id` is the rowid of the block in blocks_fts.
CREATE TABLE blocks(
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    heading_ord INTEGER,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL
);
CREATE INDEX blocks_by_file ON blocks(file_id, ord);
-- Contentless: only the inverted index; block text lives in content.pack (ADR 0008).
-- remove_diacritics folds Latin letters only, so Ukrainian й/и and ї/і stay distinct.
CREATE VIRTUAL TABLE blocks_fts USING fts5(
    text, content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
-- Pending-terms buffer before FTS5 writes a segment (default 1 MB): fewer segments and merges
-- during bulk indexing. Measured on 50k notes: FTS build 6.6 s -> 4.1 s with a larger buffer.
INSERT INTO blocks_fts(blocks_fts, rank) VALUES ('hashsize', 67108864);
-- No merging inside writes: automerge made 1 in 10 single-note updates take 10-60 ms.
-- Segments are merged by SqliteIndex::maintenance in idle time (crisismerge still bounds them).
INSERT INTO blocks_fts(blocks_fts, rank) VALUES ('automerge', 0);
-- Number of blocks per term, to recognise words found almost everywhere.
CREATE VIRTUAL TABLE blocks_vocab USING fts5vocab(blocks_fts, 'row');
";

/// Set on every connection.
pub(crate) const PRAGMAS: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
PRAGMA mmap_size = 268435456;
-- 16 MB: a cold build of 50k notes is ~3% slower than with 64 MB, and leaves ~60 MB less
-- fragmented heap behind (ADR 0014).
PRAGMA cache_size = -16384;
-- Checkpoints run in maintenance; the automatic one (16384 pages, 64 MB) is only a safety net.
PRAGMA wal_autocheckpoint = 16384;
";
