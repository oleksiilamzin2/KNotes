# 0021. Tables in the live preview

Status: accepted, 2026-11-13

## Context

Notes hold Markdown tables (`| a | b |`, a delimiter row `|---|:--:|`, Obsidian's and
GitHub's syntax). The live preview showed their source. The goal (owner's brief) is tables
edited as in Notion. The owner answered a question round (2026-11-13).

TextKit 2 lays out no tables: `NSTextTable` blocks send a text view back to TextKit 1.

## Decision

- Markdown tables only; databases over notes' properties (as Obsidian Bases) are a later
  stage (owner's choice).
- A table always shows as a grid, the cursor in its cells too, and is edited in place: the
  Markdown changes underneath (owner's choice over showing the source with the cursor in it).
- Rows and columns: "+" buttons at the bottom and the right on hover, and menus on a row and
  a column (insert above, below, left, right; delete). Keys: Tab and Shift+Tab to the next and
  the previous cell (Tab in the last cell adds a row), Return down, arrows between cells. Rows
  and columns move by dragging their handles. A column's menu aligns it (left, centre, right,
  written as `:--`, `:-:`, `--:`) and sorts the rows by it.
- A table wider than the note column scrolls sideways, its columns as wide as their content
  (owner's choice over wrapping).
- How: each row stays text in the text view. The pipes and the spaces around cells are hidden
  (as other markup) and kerning moves each cell's text to its column; the delimiter row folds
  away; the row's layout fragment draws the borders, the header's background, handles and
  buttons (as formulas, ADR 0017). Editing a cell is editing text, so the cursor, selection,
  input methods, spelling, undo, find, links and formulas work in cells as elsewhere. Edits
  that would break the structure (deleting a pipe or a row's line break) are refused; a `|`
  typed in a cell is written `\|`. Rows, columns, moves, alignment and sorting rewrite the
  table's source as one edit.
- Rejected: an `NSTableView` over the text for each table (row and column dragging, sorting
  and resizing come with it, but cells are edited in separate fields without the editor's
  features, and the view has to follow TextKit's layout as it re-estimates while scrolling).

## Consequences

- Text in a cell does not wrap; a long cell widens its column.
- Column widths are measured from the cells' shown text on every restyle of the table.
