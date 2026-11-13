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

## Built (2026-11-13)

- `highlight` gives each table's rows, cells (without pipes and the spaces around them) and
  delimiter row; `TableText` reads a table's cells and writes them back evenly (`| a | b |`,
  `| :--- | ---: |`), and adds, removes, moves, aligns and sorts rows and columns (numbers as
  numbers, text ignoring case, the header kept).
- A row's hidden pipes and spaces keep, by kerning, the room that moves its cells' text to
  their columns; a row runs on past the column instead of wrapping; padding is a taller line
  above (spacing before the document's first paragraph is dropped) and spacing below. The
  fragments draw borders and the header's background within the text column.
- Edits that touch a hidden pipe or space are refused; a `|` or a line break typed in a cell is
  written `\|` or a space, and text typed between cells goes into the nearest one, right
  after the check (changing the text inside `shouldChangeText` would confuse the text view).
  A cursor that lands between cells moves into the nearest one.
- Handles and add bars show with the pointer over the table (or near it); a handle clicked
  opens its menu, dragged moves its row or column. A right click in a cell puts its row's and
  column's actions first in the text's context menu (owner's request: deleting a row or a
  column was hard to find behind the handles). In probe mode the menu is listed or an
  item chosen instead of shown.
- A wide table scrolls by columns, not by points: kerning cannot move glyphs left of a line's
  start (a glyph's advance does not go below nothing), and shifting only the drawing would
  leave the cursor and clicks where the text is laid out. Columns scrolled away collapse to
  the table's left edge; a sideways swipe (or Shift with the wheel) moves a column at a time;
  the cursor in a hidden column scrolls it back; shadows mark the cut edges.
- A table is laid out whole on every restyle (its columns come from all its rows), but its
  other rows are set again only when its column widths change: setting a row lays it out
  again. Typing in a cell of a 200-row table (M5 Pro, release): p50 1.9 ms, p95 2.5 ms; with
  the cell widening its column on every key, p50 7.2 ms. Cell widths are measured once per
  text and font.
- The editor bench now drains an autorelease pool per step, as the run loop does per event:
  without it the cell measurements' temporary strings piled up to 300 MB in the bench.
