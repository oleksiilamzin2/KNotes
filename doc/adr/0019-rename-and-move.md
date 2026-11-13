# 0019. Renaming and moving notes, with their links

Status: accepted, 2026-11-13

## Context

Nothing in Knotes renamed or moved a note, and a note renamed elsewhere left the links to it
broken. Obsidian rewrites the links in the vault when a note is renamed inside it. The owner
answered a question round (2026-11-13).

## Decision

- Where: the note list's context menu (Rename, the title edited in its row; Move to…, a
  folder picked by search), dragging a note onto a sidebar folder, renaming a folder from the
  sidebar's context menu, and the note's name in the toolbar.
- A note's name is its file name without `.md` (owner's choice). It shows in the toolbar
  after the history buttons; clicking it edits it, Return renames the note, Escape gives the
  name back. The first line is content, as in Obsidian: renaming never changes it and
  editing it renames nothing (ADR 0011 still names a new note after its first line, once).
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
- For a few hours the first line was the name, as in Apple Notes (large and bold, renaming
  the file when edited, its tags and links not the note's). The owner reverted it the same
  day: the first line is content and the name is the file's. Indexes built meanwhile
  (version 10) are rebuilt (version 11).
- Probe actions: `rename=`, `list-rename=`, `list-move=`, `drag-to=`, `folder-rename=`, `toolbar-rename=`, with
  `KNOTES_PROBE_ANSWER` for the question; the drag gesture itself is not driven (its
  pasteboard writer and the folder's drop handler are).
- Later (2026-11-13, owner's requests): the name in the toolbar is edited only after a double
  click (a single click started renaming by accident), the name selected without a PDF's
  `.pdf`, as Finder selects it. A PDF open is named there by its file name, without its folder,
  and renamed there too: `.pdf` is kept, links to it are updated as a note's are, and the
  viewer shows it from its new place at the page it was at.
