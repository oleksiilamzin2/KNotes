# 0014. Memory after large index work

Status: accepted, 2026-11-12

## Context

ADR 0006 limits resident memory to 300 MB at 50k notes. The UI bench measured 245 MB, but
only on a warm start. After the first (cold) index of a 50k vault the app's physical
footprint stayed at 432–476 MB until it was restarted (142 MB on a warm start).

Profiling (vmmap, a headless index run with a counting allocator) found:

- macOS malloc keeps freed large blocks in a cache, dirty and counted in the footprint
  ("Malloc Large (empty)": about 250 MB). The indexer frees about 830 large Rust blocks
  (868 MB in total, at most 56 MB live at once) of many sizes, so the cache fills up. It is
  given back only under memory pressure, and `malloc_zone_pressure_relief` frees nothing
  (it returns 0). Whether some of it is returned earlier varied from run to run (136–476 MB).
- SQLite keeps its page cache (64 MB on the writer) full after a cold build, for the life
  of the connection; the freed pages leave fragmented small regions behind.
- The environment switches `MallocLargeCache=0` and `MallocSpaceEfficient=1` also fix the
  first point, but they are undocumented and only read at process start.

## Decision

- A global allocator (`knotes-alloc`, `DirectLarge`) maps blocks of 128 KB and more
  straight from the kernel and unmaps them when freed; smaller blocks use the system
  malloc. The app and the benchmark use it.
- The session frees the writer's clean page cache (`sqlite3_db_release_memory`) after a sync
  of 256 files or more and when idle maintenance has nothing left to merge.
- The writer's page cache is 16 MB instead of 64 MB.

## Consequences

- 50k notes, release, M5 Pro: footprint after a cold index 153–158 MB (was 432–476 MB);
  warm start 135 MB (142 MB before). Cold index 6.70 s (6.59 s with the 64 MB cache), every other ADR 0006
  budget unchanged.
- Each large block costs a system call and fresh zero pages; no slowdown was measurable.
- A window test compares the footprint after a cold and a warm open of an 8k-note vault
  (+33 MB now, +64 MB without the allocator, +90 MB before both fixes).
- SQLite's own large blocks still go through malloc; they were not a significant part.
