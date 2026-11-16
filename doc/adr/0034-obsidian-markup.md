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
- **Editing an embedded note in place**: a click puts the cursor in its view, which then shows
  the markup of the cursor's paragraph (none while it has not the keys) and takes edits as the
  note's own view does, with its own undo steps (the window's would undo the note around it).
  Its box grows and shrinks with its text. The edit goes back into its note 2 s after the last
  key, when the view gives up the keys, when the note around it is left, and on Cmd+S: in the
  part's place while that note is as last read or written (its own heading may have changed in
  the edit), else where the part is now (`splice_part`); a part gone from it is not written,
  and said. While an embed is edited (or has the keys) a snapshot does not read it again over
  what is typed; another box of the same note follows once written.
- **An embedded note behaves as the note itself**: its links resolve (and a missing note is
  made) from it, not from the note around it; links and tags complete in it, links written as
  from it; Cmd+F finds in it too, its occurrences in their place in the note (at its line),
  stepping through them selects them there; the note list's preview leaves embeds out.
- **Opening an embedded note**: a button at its box's top right (an arrow out of a square, dim;
  its text leaves it room) opens the note at the embedded heading or block, its edits written
  first. A heading or block (from the button or a `[[note#Heading]]` link) is scrolled to the
  top from the layout, not from where it is on screen (the window may not be on it yet).

## Later (2026-11-16): embedded notes as safe as the note (ADR 0035)

- A part changed outside while edited goes beside its note as a conflict copy before the edits
  go in, as the note's own conflicts do (ADR 0011); a change elsewhere in that note is kept.
- An embedded note gone from the disk is not written back where it was: what it would hold goes
  beside that place as a copy.
- Edits in an embed whose line is deleted or changed before they are written are still written
  (kept with the view gone, `leaving`), not dropped with it.
- An edit that cannot be written (its part gone, its note now in another encoding or not
  readable, the disk refusing) stays edited, and leaving the note asks, as for the note itself;
  a note not in UTF-8 is not embedded (it stays a link), since edited it would lose its bytes.
- Renames, moves and deletions write the embeds' edits first.
