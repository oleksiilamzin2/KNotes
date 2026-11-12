# 0003. Index storage and search

Status: accepted, 2026-11-12

## Context

Target vaults hold 10k–50k notes (budget) and up to 100k (stress test). The Markdown
files are the source of truth; the index is a rebuildable cache. Three kinds of search
are required: by file (title, path), by content, and scoped (first select a set of
files, then search content inside it). A separate MCP server process must read the
index while the app writes it.

Candidates: SQLite, DuckDB, Tantivy, plain scanning.

DuckDB was rejected:

- it is a columnar analytics engine; the workload here is many small writes (one note
  changed, its links and blocks replaced) and point lookups (resolve a link, backlinks);
- its full-text index is not updated incrementally and has to be rebuilt;
- while one process holds the database for writing, other processes cannot open it,
  which blocks the MCP server.

Tantivy would rank better and search faster than FTS5, but adds a second on-disk index
with a single-writer lock. It stays an option if FTS5 misses the budgets in ADR 0006.

## Decision

- **SQLite in WAL mode** (`rusqlite`, bundled) stores notes, folders, headings, links,
  tags, frontmatter properties and blocks. One writer thread in the app; readers in the
  app and in the MCP process.
- **Content search** uses FTS5 over **blocks**: a note is split into sections by
  headings and sections into chunks of a few hundred tokens. Ranking is bm25. Results
  are fragments with a block id; the UI groups them by note, the chat agent reads block
  ranges instead of whole notes.
- **Title and path search** runs in memory with a fzf-style matcher (`nucleo-matcher`)
  on a background thread; the list of titles is loaded from SQLite at start.
- **Exact and regex search** is a parallel scan in the style of ripgrep (`regex`,
  `memchr`, `rayon`) over the note files, streaming results as they are found.
- **Scoped search**: the scope (title/path/tag/property filter) produces a set of note
  ids; FTS5 and the scan then run only on those ids.
- The index lives outside the vault, in
  `~/Library/Application Support/Knotes/vaults/<hash of vault path>/`, so it never ends
  up in iCloud, Dropbox or git (syncing an open WAL database corrupts it). Unlike
  `Caches`, the system does not purge it, which matters once embeddings are stored.
  Data left there by Knotes v1 is not touched.

## Consequences

- One small embedded engine covers metadata, links and ranked search.
- Block-level FTS means 1–2 million FTS rows at 50k notes; index size is measured in
  the benchmarks.
- Language-aware matching (Ukrainian and English word forms) is not solved by the
  default `unicode61` tokenizer; it gets its own decision when search quality is
  measured.
- Semantic (embedding) search is not part of this decision; it can be added next to
  FTS5 later.
