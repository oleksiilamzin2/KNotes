# 0015. Search in the note list

Status: accepted, 2026-11-12

## Context

The owner asked for a search icon over the second column that searches the list. Choices
made with the owner: within the list the sidebar selected (the whole vault for "All notes");
first titles and text together, then Obsidian's operators with file names by default.

## Decision

- A magnifying glass in the toolbar over the list shows a search row at the top of the
  list column, under the toolbar; the list scrolls beneath it. A search field in the toolbar
  itself (NSSearchToolbarItem, 220 pt open) did not fit next to the list title and pushed the
  toolbar separator 100–170 pt past the list/note boundary.
- Operators as in Obsidian (owner's follow-ups): file names by default (`file:` or `files:`
  says so explicitly); `content:` finds notes holding every word anywhere in them, as
  Obsidian does; `block:` asks for every word in one block. File names: fuzzy title search,
  best first. Text: full-text search with word forms through the query stemmer, and every
  word also as the start of a word however short (`люц бап` finds "Люцифер" and
  "Баптист"), so halves of words work while typing. Each note once, best block first, the
  row showing the text around the match. Always within the selected list.
- Note-level search: each word's notes come from its own full-text match and are
  intersected; up to 1000 notes are ranked by bm25 over their matching blocks, larger sets
  show their first 500 notes unranked (ranking 7k notes took ~37 ms).
- A Files/Content switch in the search row rewrites the operator in the query, so the text
  stays the only truth; typing the operator moves the switch.
- The search runs in a background thread with its own read-only index connection; queries
  typed while it runs replace each other, and only the answer to the latest one is shown.
- Clicking the glass again hides the row and restores the list.

## Consequences

- Measured on 50k notes (M5 Pro, release): a search takes 10–40 ms in the background (the
  first title match after a pause is the slow part, ~25 ms); the main thread spends ~1.5 ms
  sending it and ~2–3 ms showing the results.
- Note-level text search at 50k (M5 Pro): p95 12.5 ms for two words, 14.4 ms for two
  three-letter word starts (about 25–29 ms on an M1, inside the 30 ms budget of ADR 0006
  with little room; the first try, filtering by temporary block tables and ranking every
  match, took 44–55 ms).
- No keyboard shortcut yet: Cmd+F is left for finding inside the open note.
