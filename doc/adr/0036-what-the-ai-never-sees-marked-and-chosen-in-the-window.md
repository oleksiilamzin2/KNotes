# 0036. What the AI never sees: marked, and chosen from the menus

Status: accepted, 2026-11-16.

## Context

ADR 0004 decided, and ADR 0027 built, that a note tagged `#no-ai` and a folder whose
`.knotes.json` says `"ai": false` are never shown to the AI (the chat and the MCP server). It
could only be said by hand: a tag typed, a JSON file written. Nothing in the window showed what
was kept, and a PDF (which has no tags) could be kept only with the whole folder it was in.

The owner asked (2026-11-16) for folders and files to be kept from the AI from the window,
those kept marked with a small grey struck-through brain.

## Decision

- **A note is kept by its tag, as before.** «Сховати від ШІ» in a note's menu (the list) puts
  `no-ai` in its frontmatter's `tags`, in the form they have (`[a, b]`, `- a` lines, words become
  a list), a frontmatter made when it has none. The tag stays the one way for a note: it goes
  with the note wherever it is moved, copied or synced, Obsidian shows it, and the AI's tools
  read it from the index.
- **Another file is kept in its folder's rules file**: `"files": {"a.pdf": {"ai": false}}`. A
  file cannot carry a tag, and its folder's file goes with it through iCloud or git. `"ai":
  true` there shares a file (a note too) its folder keeps.
- **A folder is kept as before**, `"ai": false` in its own rules file, with its subfolders;
  `"ai": true` shares one inside a folder kept.
- **The nearest setting wins**: a note's tag, then the file's own setting, then its folders'
  from the nearest up. «Показати ШІ» undoes what keeps it: the tag goes (from the frontmatter and
  from the text, `#no-ai/…` with it), and where a folder still keeps it, the file or the
  subfolder says `"ai": true`. A setting is written only where it differs from its folder's,
  and a key goes when it says what the folder says already.
- **The mark shows what the AI never sees, whatever the reason**: a struck-through brain (SF
  Symbols has no `brain.slash`, so the slash is drawn over `brain`, with the gap its own
  slashes have), in the secondary label colour, after a note's title in the list and after a
  folder's or a tab's name in the sidebar, before a space's dot. A note in a folder kept is
  marked too. Hovered, it says «Сховано від ШІ».
- **The window reads it as the tools do** (`KeptFromAi`, in knotes-spaces, used by both): a
  folder whose rules file cannot be read, or names `"ai"` but does not parse, is marked kept.
  Such a file is not written over by the menu, which says what is wrong instead (ADR 0035).
- The note open is tagged in the editor, as typed (Undo takes it back), and saved at once;
  another is rewritten on the disk, edits typed in an embedded copy of it written first.
- **A file's own setting goes with it, as a note's tag does.** Renamed or moved in Knotes, its
  entry leaves its old folder's rules for its new one's (else a PDF kept, once renamed, was
  shared without a word). One kept is not moved where the new folder's rules cannot say so
  (unreadable, or not parsing): the user is told why. Deleted, its entry stays, so that it is
  kept again once restored; restored under another name (its own taken meanwhile), the new name
  is kept as well. A folder's rules are inside it and go with it anyway.

## Consequences

- Keeping something from the AI, and seeing what is kept, no longer needs JSON or a tag typed.
- A file renamed or moved outside Knotes (in the Finder) leaves its entry behind, naming a file
  no longer there, and is shared where it went; a note's tag goes with it wherever.
- The marks follow the snapshot: they show once the watcher has seen the write (well under a
  second); the AI's tools look again at most every 10 s (ADR 0027).

## Built (2026-11-16)

- knotes-spaces: `KeptFromAi` (`holds`, `holds_folder`, `tagged`, `own_file`, `own_folder`;
  `add_rules_text` keeps a folder whose rules name `"ai"` but do not parse), and `set_folder_ai`
  and `set_file_ai` for the rules text. knotes-tools' `Kept` and the session's `Snapshot::ai`
  are built with it.
- knotes-editor: `with_tag` and `without_tag` put a tag into a note's frontmatter and take it
  out of it and of the text.
- The app: `ai_mark` draws the mark and lays it out with the space's dot (an `NSStackView`, so
  a hidden mark takes no room); the list's and the sidebar's menus call `toggle_ai` and
  `toggle_folder_ai` (`vault_window/ai.rs`). Probe mode prints `list-ai-marks:` and
  `sidebar-ai-marks:` and takes the actions `list-ai`, `folder-ai=Folder` and `ai-path=path`.
- A file's own setting is carried by `carried_ai` (checked before a rename or move in
  `relocate`, written after it by `carry_ai`; copied on a restore under another name), read and
  set by `file_ai` and `set_file_ai`, which find a name written composed or not. Probe mode
  takes `delete-path=path`.

## Fixed (2026-11-18): two ways a kept document was shared

An audit of the whole repository found both; neither had a test.

- **An index that did not answer shared everything.** The tools find out what is kept from
  the index (the notes tagged `#no-ai`, the folders there are) and remember it ten seconds.
  A query that failed (the index busy past its wait, or being built again under a running MCP
  server) was taken for "none tagged, no folders": every kept note was shared until the next
  look. Now everything is kept until the index answers (`Kept::everything`), as ADR 0027 said
  of this lookup and only its reading of the rules files did.
- **An `ai` that was neither true nor false shared its folder.** `"ai": "false"` in quotes,
  `0` or `null` were passed over as if nothing were said. They keep now, a folder's and a
  file's own alike: the user wrote it to say something of the AI, and kept by mistake is
  better than shared by mistake (the rule a rules file that does not parse already followed).
