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

## Consequences

- Keeping something from the AI, and seeing what is kept, no longer needs JSON or a tag typed.
- A PDF kept loses that when it is moved to another folder (its entry stays behind, naming a
  file no longer there); a note's tag goes with it.
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
