# 0019. Renaming and moving notes, with their links

Status: accepted, 2026-11-13

## Context

Nothing in Knotes renamed or moved a note, and a note renamed elsewhere left the links to it
broken. Obsidian rewrites the links in the vault when a note is renamed inside it. The owner
answered a question round (2026-11-13).

## Decision

- Where: the note list's context menu (Rename, the title edited in its row; Move to…, a
  folder picked by search), dragging a note onto a sidebar folder, renaming a folder from the
  sidebar's context menu, and the note's first line (below).
- The first line is the note's name, as in Apple Notes (owner's choice): the first line of
  text after the frontmatter shows in a large bold font, its `#` markers hidden. Editing it
  renames the file when the line is finished with Enter or the note is left (the moment ADR
  0011 names a new note); later edits rename again (this replaces ADR 0011's "once").
  Opening a note never renames it. Renaming from the list rewrites the first line when it
  was the old name; a first line that is something else stays as it is.
- Tags and links written in the title are the name's, not the note's (owner's choice): they
  show as plain title text (no colour, no click), the index does not count them, and the
  file name takes the title's words without tags and with the text links show. A first line
  of tags and links alone names nothing (Obsidian notes often start with a line of tags),
  so it is no title and its tags stay the note's. The open note's name shows in the
  toolbar over the note; clicking it edits it and Return renames the note.
- Links: before a rename that changes links in other notes, the app asks "Update N links in
  M notes?" (Update / Don't update / Cancel). Updating keeps each link's form: a bare name
  stays bare while the new name is unique, otherwise it gets the full path; a full path stays
  full; `.md`, `#heading`, `^block` and `|alias` stay; Markdown links get the new path in the
  form they had (relative to the note or from the vault's root). A moved note's own relative
  Markdown links are rewritten to keep pointing where they did.
- Files move with an exclusive rename (no file is replaced); edited notes are written
  atomically. The open note, if edited by the update, is saved first and reloaded after.

## Consequences

- A rename touching many notes writes them all; the index catches up through its watcher.
- Renames done outside Knotes still break links, as in Obsidian.
- An old note whose first line is a paragraph shows it large, as Apple Notes would.

## Built (2026-11-13)

- `knotes_app::relink` rewrites every link of a note for a set of moves in one pass over the
  original spans (links to moved files, and a moved note's own relative Markdown links);
  `plan_relinks` finds the notes to edit through the index's backlinks and parses their
  current text again, since the index may lag behind it.
- A bare name stays bare when it still opens the note under the resolution rules of ADR
  0009 (the shortest path wins among notes of one name), not only when it is unique.
- Move to… picks the folder in a combo box of the vault's folders that completes as it is
  typed (a native dialog; a typed new folder is made), instead of a separate search panel.
- A renamed folder moves in one rename, attachments and files outside the index included;
  links to all its indexed files (images too) are updated.
- The title is the first line of the first block when that is a heading or a paragraph; a
  note starting with a list, code or a quote has none.
- Probe actions: `rename=`, `list-rename=`, `list-move=`, `drag-to=`, `folder-rename=`, with
  `KNOTES_PROBE_ANSWER` for the question; the drag gesture itself is not driven (its
  pasteboard writer and the folder's drop handler are).
