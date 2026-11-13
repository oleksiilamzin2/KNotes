# 0018. Live preview of lists, tasks, quotes and code blocks

Status: accepted, 2026-11-13

## Context

After formulas (ADR 0017) the live preview still showed list markers, task boxes, quote
markers and code fences as source, and struck text only dimmed. Obsidian's live preview is
the reference (ADR 0011), with one change the owner asked for.

## Decision

- Struck text: a line through it, in the text colour.
- Lists: `- `, `* `, `+ ` show as a dot; numbers show, in digits of one width. The source's
  indentation is hidden and each level indents the same (20 pt), wrapped lines under the
  item's text. A marker shows its source only while the selection touches it (as in
  Obsidian), not for the whole line, so typing in an item keeps its dot or box.
- Tasks: circles, not Obsidian's squares (owner's choice): outlined, or filled in the accent
  colour with a tick. A click on one ticks or unticks it through the text view (undoable,
  saved; the cursor stays). Done tasks' text is muted and struck through.
- A space typed after a list item's `- []` makes it `- [ ] `, a task: `[]` is no task in
  Markdown or Obsidian, but it is what one types for one. A paste stays as it is.
- Quotes: a bar per level down the paragraph instead of `>`, the text indented past them;
  `>` shows with the cursor on the line, like other markup.
- Code blocks: one background across the column, rounded at the block's ends, the code
  inset in it; fence lines fold to padding unless the cursor is in the block.
- Links to websites (owner's request, as Obsidian marks them): an arrow ↗ in the link colour
  after the link's text while its `](url)` is hidden; links inside the vault have none.
- All of these are drawn by the paragraph's layout fragment, as formulas are, in the room
  the hidden source keeps (kerning, paragraph styles); attributes are composed in memory
  first (ADR 0012).

## Consequences

- A hidden source line has no height of its own in TextKit 2 (it takes it from the hidden
  glyphs): empty quote lines get a minimum line height, folded fences a fixed one.
- Not done yet: inline code background, Obsidian callouts (`> [!note]`), code language
  label and copy button, task text muting for links.

## Measured 2026-11-13 (M5 Pro, release)

| Note | Open | Full restyle | Keystroke p50 |
|---|---|---|---|
| 981 KB, 6000 list items and tasks | 31 ms | 17 ms | 5.6 ms (5.3 ms before; highlighting the whole note) |
| 611 KB, ~1300 formulas | 19 ms | 4.7 ms | 2.2 ms |
