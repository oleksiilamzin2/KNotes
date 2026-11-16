# 0034. The rest of Obsidian's markup: highlights, comments, footnotes, embedded notes

Status: accepted, 2026-11-16.

## Context

A vault written in Obsidian uses markup Knotes showed as typed: `==highlights==`,
`%%comments%%`, footnotes `[^1]` and embedded notes `![[note]]`. ADR 0007 promises Obsidian's
vaults read as they do there. The owner chose (2026-11-16): comments dim (as Obsidian's live
preview), footnotes as a raised number that a click follows, embedded notes in a frame that can
be edited in place.

## Decision

- **Highlights**: `==text==` within a line, outside code, formulas and comments; a sign before a
  non-space opens, the next after one closes; three or more `=` are no sign (a heading's
  underline). Shown on yellow, keeping the font and colour of what they cover (a heading's
  stays bold); the signs hidden away from the cursor, as other markup.
- **Comments**: `%%…%%`, inline or across lines, outside code, formulas and frontmatter; one
  left open runs to the end of the note, as in Obsidian. Shown dim, the signs hidden away from
  the cursor, nothing styled inside. Their tags, links, anchors and headings are not the
  note's (index, outline, connections), as in Obsidian. The note list's preview leaves them
  out, and a highlight's signs.
- One scan for each in knotes-md (`comment_ranges`, `mark_ranges`), shared by the index, the
  preview and the editor.
- **Footnotes** (pulldown-cmark's, GFM's: a reference with no definition stays text): a
  reference `[^label]` shows its label raised, smaller, in the link colour, its brackets hidden
  away from the cursor; a definition keeps `[^label]:` in view, muted. A click on a reference
  away from the cursor's paragraph (anywhere with Cmd) puts the cursor after its definition's
  `]:`; a click on a definition's label, after its first reference. Labels match ignoring case.
- **Embedded notes**: `![[note]]` alone on its line, resolving to a note of the vault, shows
  the note without its frontmatter; `![[note#Heading]]` its section (the heading to the next
  one as high); `![[note#^id]]` its block (the paragraph, a list item alone, the anchor left
  out). The line's source hides away from the cursor and keeps a box's room (a tinted rounded
  box, drawn by its layout fragment); with the cursor on it the source shows and the box goes
  below it. The note shows in a text view of its own over the box, styled by a live preview of
  its own, with nothing revealed while it is read. These views are the note view's subviews,
  so they scroll with it with no lag (ADR 0017's layers lagged, being over it); after each
  layout of the note (its viewport controller's `didLayout`) they follow their boxes. An
  embed inside an embedded note stays a link (no recursion); a note embedding itself too.
  Typing reads no embedded note again: only an embed new on the page, or the notes a snapshot
  says changed, are read; a new column width measures them again after the layout.
  Editing in place comes next.
