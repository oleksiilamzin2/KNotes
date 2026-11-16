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
