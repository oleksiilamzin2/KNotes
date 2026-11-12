# 0006. Performance budgets

Status: accepted, 2026-11-12

## Context

The goal is a harness that stays fast on a large knowledge base, for three users at
once: the app UI, the in-app chat agent, and external agents through MCP. Budgets are
set for 50k notes on an M1; 100k notes is the stress case, allowed to be up to 2×
slower but never to freeze. Development runs on an M5 Pro, so measured numbers there
must be at most half the M1 budget.

## Decision

Budgets at 50k notes (about 250 MB of Markdown), M1, warm page cache:

| Operation | Budget |
|---|---|
| Title/path fuzzy search, per keystroke, p95 | 5 ms |
| Content search (FTS5), top 50 blocks, p95 | 30 ms |
| Regex scan of the whole vault: first results / complete | 50 ms / 300 ms |
| Scoped search | proportional to scope size, never above the unscoped budget |
| Open a note up to 100 KB (read, parse, first frame) | 16 ms |
| Reindex one changed note (off main thread) | 10 ms |
| Cold full index, in background | 20 s, UI usable meanwhile |
| Warm start: change detection over all files | 1 s |
| Main thread work per frame besides layout and drawing | 4 ms |
| Idle CPU | ~0 %: no polling timers, redraw only on change |
| Resident memory | 300 MB |

Benchmarks run on synthetic vaults (10k, 50k, 100k notes, continuous size distribution
from one line to tens of pages, Ukrainian and English) and on the owner's real vault,
read-only.

## Consequences

- Every stage that touches a budgeted path adds or updates a benchmark.
- A missed budget blocks the stage until it is fixed or the budget is renegotiated
  with the owner in a new ADR.
