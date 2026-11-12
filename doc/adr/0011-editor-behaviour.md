# 0011. Editor behaviour

Status: accepted, 2026-11-12

## Context

Stage 3 builds the note editor (ADR 0005: live preview on TextKit 2). Before building on
TextKit 2, its riskiest parts are checked in a spike. The owner decided how editing behaves.

## Decision

- Start with a TextKit 2 spike: hide Markdown markup outside the cursor line, scroll a
  300 KB note at 120 fps, intercept keys (for the later vim stage).
- Live preview renders everything in this stage: headings, emphasis, code, links and
  wikilinks (click, autocomplete), lists, task boxes, quotes, tags, formulas, images, PDFs
  and Notion-like tables.
- Autosave 2 s after the last edit, and at once when another note opens, the window closes
  or the app quits; Cmd+S saves now. Writes are atomic (temporary file and rename).
- A new note (toolbar, Cmd+N) is created in the selected folder as `Без назви.md`; after a
  pause its file takes the name of the first line (or `#` heading), once. Later edits of
  that line do not rename the file.
- Renaming or moving a note rewrites every link to it in other notes, found through the
  backlinks of the index.
- If a note changes on disk while the editor has unsaved edits, both versions are kept:
  the editor's in the note, the outside one next to it as
  `<name> (конфлікт YYYY-MM-DD HH.MM).md`, with a notice. Without unsaved edits the
  outside change simply reloads.
- vim mode is a separate later stage.

## Consequences

- No edit is ever lost silently; conflict files may need manual merging.
- Link rewriting touches other files; until the git stage it has no undo beyond the
  editor's own.
