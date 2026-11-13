# 0020. Images in notes

Status: accepted, 2026-11-13

## Context

Notes embed images as Obsidian does: `![[picture.png]]`, `![[picture.png|300]]` or
`|300x200`, and `![alt](path/picture.png)`. The live preview showed them as links. The
owner answered a question round (2026-11-13).

## Decision

- An image embed alone in its paragraph shows the image, centred (owner's choice), at its
  natural size but no wider than the note column, or at the size written after `|`. Away from
  the cursor its markup is hidden; a click puts the cursor in it, as with display formulas
  (ADR 0017): the markup shows and the image stays below it.
- Dragging the image's right edge resizes it and writes the width into the embed (`|300`).
- A double click opens the image in Quick Look.
- An image pasted (Cmd+V) or dropped from Finder into a note is copied to
  `.knotes/attachments` at the vault root as `Pasted image 20261113114500.png` (Obsidian's
  naming) and embedded where the cursor is. Obsidian ignores folders starting with a dot, so
  it does not show these images (owner's choice: the vault stays clean).
- `.knotes/` at the vault root becomes Knotes's folder in the vault (as `.obsidian/` is
  Obsidian's). The per-folder rules file of ADR 0010 is renamed from `.knotes` to
  `.knotes.json`; an old `.knotes` file is still read and is replaced on the next write.
- The scan and the watcher take in `.knotes/attachments`; the sidebar shows no hidden folder.
- No images from the web (`![](https://…)` stays a link): no network requests from notes.

## Consequences

- Inline images inside a line of text stay links for now.
- Images pasted in Knotes are broken links in Obsidian.
