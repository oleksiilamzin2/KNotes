# 0031. Folders made in the app, and empty folders shown

Status: accepted, 2026-11-15.

## Context

A folder could only be made by moving a note into a path typed for it (ADR 0019) or in the
Finder, and the sidebar built its tree from the indexed files, so an empty folder never showed.
Obsidian shows every folder. The owner asked how to make a folder and a subfolder, and chose to
show empty folders as Obsidian does.

## Decision

- **Every folder shows**: the vault lists its folders (`FolderLister`, knotes-core; `FsVault`
  walks them reading only its entries' types, hidden folders and the attachments folder left
  out), and the folder tree takes them beside the files' (`folder_tree_with`), empty ones with no
  notes. A snapshot lists the folders again only when a change is not a known file's (a folder
  came or went, or a rules file changed); otherwise it keeps the previous list, so editing notes
  costs no walk.
- **Making one**: File → «Нова тека» (Shift+Cmd+N), in the folder selected or at the top; a
  folder's menu → «Нова підтека»; the sidebar's menu below its rows → «Нова тека». The folder is
  made at once as «Нова тека» (numbered from 2 when taken, as Finder does), shown before the
  watcher's snapshot comes, its parents opened, and its name edited in its row; Escape keeps
  «Нова тека». Made at the top while the window is in a space, it joins the space (ADR 0030).
- **While a folder's name is edited the sidebar waits**: a snapshot (certain after a folder is
  made) would end the editing; the sidebar is rebuilt when it ends. The field holds the folder's
  own name though its row may show a path (a folder hanging away from its parent in a space).

## Consequences

- A rules file's change (`.knotes.json`) walks the folders again: rare enough.
- Deleting a folder is not in the app yet.
