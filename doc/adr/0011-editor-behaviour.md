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
  Refined when built (2026-11-12): "after a pause" means once the first line is finished, by
  a line break after it or by leaving the note (another note opens, the window closes, the
  app quits): a 2 s pause mid-line would cut the name where the user stopped to think.
  Markup and characters that break file names or links are left out (`/` becomes `-`), at
  most 100 characters, a number is added when the name is taken. A new note left empty
  keeps its `Без назви.md` file: nothing is deleted behind the user's back.
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

## Later (2026-11-16): what the promise needed (ADR 0035)

- "No edit is ever lost silently" did not hold where a write failed: autosave now says so once,
  the edits stay in the window, and leaving the note, closing the window or quitting asks to
  stay with them or discard them.
- Writes keep the replaced file's permissions, creation date, extended attributes and ACL, and
  reach the disk before the rename.
- A note deleted or moved outside Knotes while open closes instead of being written back where
  it was; edits not written yet go beside that place as a conflict copy. A version from outside
  whose conflict copy cannot be written is not overwritten.
- A note not in UTF-8, or one that cannot be read, opens read only (it was read lossily, or
  opened empty and editable, and a key typed replaced the file).
- A new note named after its first line keeps its tab (ADR 0024); if the name cannot be given,
  that is said once and the note keeps `Без назви.md`.
