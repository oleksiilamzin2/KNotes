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
  the front window's vault; entries older are deleted for good when a vault opens and then
  daily while it is open.
- **In the window**: «Видалити» in a note's menu in the list (Cmd+Backspace) and in a folder's
  in the sidebar (a folder with notes asks first); «Нещодавно видалені (N)» at the sidebar's
  foot while there is any, listing the entries newest first (each with where it was and when
  it went), with «Відновити» and «Видалити назавжди», and «Очистити» for them all. A note open
  when deleted closes; its tab goes.
- One trait, `TrashStore` (knotes-core), `FsVault` keeping it.

## Consequences

- Links to a deleted note show as missing until it is restored.
- What is in the trash takes room in the vault, and in git if the vault is a repository, until
  its days pass.
