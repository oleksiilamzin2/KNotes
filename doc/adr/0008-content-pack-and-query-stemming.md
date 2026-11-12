# 0008. Content pack for scanning, query-side stemming for word forms

Status: accepted, 2026-11-12

## Context

Measured on an M5 Pro with the 50k synthetic vault (268.7 MB): reading all files in
parallel takes 250 ms before any matching is done, so a regex scan over files would
take 500–600 ms on an M1, above the 300 ms budget in ADR 0006. The chat agent also
reads block ranges often, and opening files for each read adds latency.

Ukrainian is highly inflected (мова, мови, мовою); exact-word search misses most
forms. Knotes v1 chat lost answers on exactly this.

## Decision

Content pack:

- All note texts are copied into one append-only file, `content.pack`, next to the
  SQLite index. The `notes` table stores each note's offset and length in the pack.
- A changed note is appended again; the old range becomes dead. The pack is compacted
  (rewritten and atomically renamed) when dead bytes exceed a threshold.
- Readers (search threads, the MCP process) map the pack read-only with `mmap` and
  remap when it grows or is replaced.
- Regex scans, snippets and block reads use the pack; the Markdown files stay the
  source of truth.
- FTS5 is contentless (`content=''`, `contentless_delete=1`): it keeps only the
  inverted index; text for snippets comes from the pack.

Word forms:

- The FTS5 index uses the plain `unicode61` tokenizer (diacritics folded).
- At query time each word is reduced by a light Ukrainian/English suffix stripper and
  searched as a prefix (`мовою` → `мов*`). Results containing the exact word rank higher.

## Consequences

- About +270 MB of disk at 50k notes; the expected scan time is 30–60 ms.
- Prefix matching brings some false hits (`мовчання` for `мова`); quality is measured
  and an index-side stemmer stays the fallback.
- Pack and index must stay consistent; the pack generation is recorded in SQLite.

## Update 2026-11-12: generations and compaction

- The pack starts with a 16-byte header: magic `KNPACK01` and a generation number. The
  same generation is stored in the SQLite `meta` table; `open` rebuilds the index when they
  differ or the header is missing.
- Compaction writes the live texts to `content.pack.new` (synced), moves the offsets and
  bumps the generation in one transaction, and renames the new file over the old one
  before the commit. A crash at any step leaves different generations, so stale offsets
  are never used. Snapshots taken earlier keep their mapping of the old file.
- Idle maintenance compacts when at least 16 MB are dead and dead bytes outnumber live
  ones. Measured on 50k notes with half the pack dead: 537 MB -> 269 MB in 157 ms.
- Other processes (the MCP server) must compare the generation they mapped with the one in
  SQLite inside the same read transaction and remap on change.
