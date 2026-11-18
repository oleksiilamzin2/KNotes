# 0032. Recently deleted: notes, PDFs and folders kept to be restored

Status: accepted, 2026-11-16.

## Context

Knotes could not delete anything: a note went only through the Finder, for good. The owner
asked for a trash of the app's own, which keeps what folders and notes say of themselves so
that a restore brings it all back, its days kept 30 by default and set in the settings. Apple
Notes keeps deleted notes 30 days in «Нещодавно видалені»; Obsidian moves them to `.trash` in
the vault.

The owner chose (2026-11-16): kept in the vault; the days set per vault; named «Нещодавно
видалені»; notes, PDFs and folders deleted alike.

## Decision

- **Kept in the vault**, in `.knotes/trash` (Knotes's folder, ADR 0020): it travels with the
  vault (git, sync), so what one Mac deleted another restores. Hidden, so the index, search, the
  chat and the watcher leave it out, as Obsidian's `.trash`.
- **An entry a deletion**: `.knotes/trash/<deleted ms>-<n>/` holds the file or the whole folder
  as it was (moved, not copied: its text, frontmatter, dates and, for a folder, its
  `.knotes.json` and its subfolders' stay as they were) and `entry.json`: where it was, whether
  it is a folder, when it was deleted, how many notes it holds, and the rules files of the
  folders above it (their space, list settings), so that a restore after those folders went
  makes them again as they were.
- **Restore**: back to where it was, the folders above made again with their rules; another
  name beside it (" 2", " 3") when the place is taken.
- **The days**: `"trash": {"days": 30}` in the root's `.knotes.json`, set in the settings for
  the front window's vault; entries older are deleted for good when a vault opens, when its
  root's rules change (a new number of days so applies at once) and otherwise at most hourly
  after a change (they expire with time, not with changes). Each snapshot reads only the
  entries it does not know yet: an entry does not change once made.
- **In the window**: «Видалити» in a note's menu in the list and in a folder's in the sidebar (a
  folder with notes asks first; no Cmd+Backspace: in the editor it deletes to the line's start,
  and a menu's key would take it there); «Нещодавно видалені (N)» at the sidebar's
  foot while there is any, listing the entries newest first (each with where it was and when
  it went), with «Відновити» and «Видалити назавжди», and «Очистити» for them all. A note open
  when deleted closes; its tab goes.
- **An entry chosen there is shown** where it is kept, to be read: a note's text read only, a
  PDF in its viewer, a folder in words (its name, its notes, restore it to open them). No tab,
  no history, nothing written: an edit would bring a note back where it was, so a note is
  restored to be edited (as Apple Notes asks).
- One trait, `TrashStore` (knotes-core), `FsVault` keeping it.

## Consequences

- Links to a deleted note show as missing until it is restored.
- What is in the trash takes room in the vault, and in git if the vault is a repository, until
  its days pass.

## Later (2026-11-16, ADR 0035)

- A deletion, restore or deletion for good that fails is said (one sheet for several), instead
  of leaving the entry or the note where it was without a word. A note whose edits cannot be
  written is not deleted when the user stays with them.
- A deletion that fails leaves no entry behind (an entry without `entry.json` is never listed,
  so it was never deleted for good either); a restore counts as done once the file is back,
  whatever is left of its entry, whose `entry.json` at least goes, so it is not listed with
  nothing in it.
- Deleting writes the edits typed in an embed of the note first, so they go with it.

## Fixed (2026-11-18)

A restore read where an entry goes back from its `entry.json` and used it unchecked (an
audit's finding 13): a path with `..`, or an absolute one, in an entry damaged or come from
elsewhere with the vault's sync, put the file outside the vault. It is checked now as every
path Knotes is given (`FsVault::inside`), and such a restore fails.

A new note with a first line typed and no name yet could not be deleted (finding 15):
closing its tab left it, which named its file after its line, and the delete then looked for
the old name. It is named first and deleted under that name.
