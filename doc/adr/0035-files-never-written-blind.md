# 0035. Files are never written blind: no edit or file lost without a word

Status: accepted, 2026-11-16.

## Context

ADR 0011 promised that no edit is ever lost silently, and that the editor never rewrites what
the user did not touch (ADR 0005). A review of everything that writes the vault (2026-11-16,
the owner's goal now being to bring what exists to a good state, not to add) found eleven ways
that promise failed, six of them reproduced on copies of the household vault:

- a PDF renamed or moved from the note list (search results list PDFs with a note's menu)
  became `Contract.md` or `doc.pdf.md`, and one key typed in it wrote its bytes over as text;
- a save that failed (folder read only, disk full, volume gone) only wrote to stderr, and
  closing the window or opening another note dropped the edits;
- a note not in UTF-8 was read lossily, and one key typed replaced its other bytes with U+FFFD;
- every save renamed a new file over the note: its creation date, extended attributes (Finder's
  tags) and permissions went (a `600` note became `644`); nothing reached the disk before the
  rename;
- choosing a sort order wrote `{"list": …}` over a folder's `.knotes.json` that did not parse
  (a comma too many), taking its space and `"ai": false` away;
- a new note named after its first line lost its tab;
- a note deleted or renamed outside Knotes while open was written back where it was by
  autosave; one that could not be read opened empty and editable, and a key typed replaced it;
- embedded notes (ADR 0034) wrote over a part changed outside, wrote a note gone from the disk
  back where it was, and dropped what was typed when the embed's line went before the write;
- a rename missed links just typed (written by the rename itself, not indexed yet) and rewrote
  other notes' untouched Markdown links (`./x.md`, case, `%xx`);
- every other file operation that failed (delete, restore, move, a new note or folder, a pasted
  image, a space, the settings, opening a vault) said nothing.

## Decision

- **A write that fails is said, and its edits stay.** Autosave tells it once (in a sheet,
  again only after a save has worked); the edits stay in the window. Leaving the note (another
  note or PDF, closing its tab, deleting it, a new note), closing the window or quitting asks
  «Залишитися» (the default: nothing moves) or «Відкинути зміни». Embedded notes' edits count
  as the note's.
- **What cannot be written back exactly is read only.** A note not in UTF-8 shows as well as it
  reads and is never written; renames, links made from mentions and embeds leave such notes as
  they are. A note that cannot be read (offline in iCloud, not allowed) opens empty and read
  only, with the reason. Both become editable once the file reads as UTF-8 again: when it
  changes, or when the note is opened again.
- **A note gone from the disk is not made again.** The note open, deleted or moved outside
  Knotes, closes; edits not written yet go beside where it was as a conflict copy, with a
  notice that opens it. The same for an embedded note.
- **A version from outside is never overwritten without its copy.** As ADR 0011 for the note,
  so for an embedded part changed outside while edited (a change elsewhere in that note is
  simply kept); and a conflict copy that cannot be written stops the overwrite: the edits stay
  unsaved, and leaving asks.
- **A file replaced keeps what is not its text.** The temporary file takes the old one's
  permissions, creation date, extended attributes and ACL before it is renamed over it, and
  its text reaches the disk first (`fsync`, not the drive-cache flush `F_FULLFSYNC`: that costs
  milliseconds a file, and a rename rewrites many notes on the main thread; SQLite chooses the
  same by default). Notes and `.knotes.json` files alike.
- **A rules file is written as it is on disk**, never from a snapshot's copy, and one that does
  not parse, or cannot be read, is not written over: the user is told what is wrong with it.
- **Every file operation the user asked for says when it fails**, with the reason and the files
  (one sheet for several). A rename whose links cannot be found is called off rather than
  breaking them. Knotes's own data (tabs, settings, the window's space) stays on stderr.

## Consequences

- An edit, a version from outside or a file is lost only when the user chooses «Відкинути
  зміни».
- More sheets and conflict copies where there was silence; copies may need merging by hand.
- A note in another encoding cannot be edited in Knotes until it is saved as UTF-8 elsewhere,
  and its links are not updated by renames.
- Notes written outside in another encoding while edited in Knotes still get a lossy conflict
  copy (left: no tool was found that does it).

## Built (2026-11-16)

- `FsVault::read_utf8` reads what is to be written back (`read_text` stays lossy, for the index
  and for showing); the editor reads through `read_for_editor` (`NoteRead::Text`, `NotUtf8`,
  `Failed`), and `Editing` keeps `read_only`.
- `write_edits`, `write_embeds` and `write_all` give the first failure (`SaveFailure`);
  `leave` asks (`ask_discard`) and gives false to stay: `open_note`, `open_pdf`, `show_nothing`,
  `close_tab`, `delete`, `new_note`, `windowShouldClose:` and `applicationShouldTerminate:` keep
  what is open when it does.
- `replace_file` in knotes-vault keeps the metadata (`fcopyfile` with `COPYFILE_ACL |
  COPYFILE_XATTR`, not `COPYFILE_STAT`, which would bring the old modification time back; the
  creation date through `FileTimesExt::set_created`) and syncs: 0.24 ms a note rewritten (M5 Pro).
- `NoteEmbeds` keeps embeds gone from the note with unwritten edits (`leaving`) until written,
  and `save` returns the copies it kept and the first failure (`EmbedsSaved`).
- `rename_dialog::tell_failure` says a failure in a sheet; in probe mode `failed: title: why`.
  Probe mode also prints `save-failed:`, `unsaved-question:` (answered by `KNOTES_PROBE_ANSWER=
  discard`), `read-only:`, `unreadable:`, `vanished-notice:` and `rules-not-written:`; a window
  kept open says `window stays open`, a quit called off `quit called off`.
- New probe variables: `KNOTES_PROBE_DELETE` (the open note deleted outside),
  `KNOTES_PROBE_EMBED_OUTSIDE`, `KNOTES_PROBE_EMBED_GONE`, `KNOTES_PROBE_EMBED_CUT`, and the
  actions `list-rename-path=` and `drop-path=` (a PDF is listed only among a search's results,
  which come after the probe's actions).
- Each case has a test: the window's (a read-only folder made with permissions `555`, a file
  with `000`, a note in Windows-1251, outside writes and deletions through the probe), and
  knotes-vault's and knotes-app's for what lies below the window.
