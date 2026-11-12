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
- Operators as in Obsidian (owner's follow-up): file names by default (`file:` or `files:`
  says so explicitly), `content:` searches the notes' text. File names: fuzzy title search,
  best first. Content: full-text search over blocks, word forms through the query stemmer,
  best block first, each note once, the row showing the text around the match instead of
  the note's start. Always within the selected list.
- A Files/Content switch in the search row rewrites the operator in the query, so the text
  stays the only truth; typing the operator moves the switch.
- The search runs in a background thread with its own read-only index connection; queries
  typed while it runs replace each other, and only the answer to the latest one is shown.
- Clicking the glass again hides the row and restores the list.

## Consequences

- Measured on 50k notes (M5 Pro, release): a search takes 10–40 ms in the background (the
  first title match after a pause is the slow part, ~25 ms); the main thread spends ~1.5 ms
  sending it and ~2–3 ms showing the results.
- No keyboard shortcut yet: Cmd+F is left for finding inside the open note.
