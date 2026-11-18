# 0010. AppKit shell

Status: accepted, 2026-11-12

## Context

Stage 2 builds the native window on top of the core (ADR 0002, 0003). The owner chose:
macOS 27 only; a sidebar with a folder tree and a tag section; a note list sorted by
modification time and grouped by date, with view settings remembered per folder in a
`.knotes` file inside that folder; one window per vault, the last vault reopened; the app
named Knotes with bundle id `app.knotes.Knotes`.

## Decision

- `lib/knotes-app`: view models in plain Rust (folder tree, tag tree, note list with date
  groups, per-folder settings, selection). No AppKit types, fully unit-tested.
- `bin/knotes`: the AppKit layer through `objc2`/`objc2-app-kit`: `NSSplitViewController`
  with sidebar (`NSOutlineView`), note list (`NSTableView`, view-based, only visible rows
  asked for), note view (read-only `NSTextView` in this stage) and a chat placeholder.
- Indexing runs on a background thread (sync on open, then the FSEvents feed); results
  reach the main thread as whole new view-model snapshots, never partial state.
- One vault per window; a lock file in the vault's index directory keeps a second window
  or a second app instance from opening the same vault.
- The app bundle is assembled by a script (no Xcode on the build machine):
  `Info.plist` with `LSMinimumSystemVersion` 27.0.
- A test command channel (debug feature, Unix socket) lets checks drive the window and
  read its state; window snapshots by window id.

## Consequences

- The UI logic that can break is tested without a window; the AppKit layer stays thin.
- `.knotes` files are hidden, so the scanner never indexes them, and they travel with the
  vault (git, iCloud). The watcher ignores hidden paths today and must learn to report
  `.knotes` changes.

## Later

- The test command channel became the probe mode: `KNOTES_PROBE=1` and `KNOTES_PROBE_*`
  variables drive the window once its vault is shown, and it prints its state and quits
  (`bin/knotes/tests/window.rs`); there is no socket.
- 2026-11-16 (ADR 0035): a list option is written into the folder's rules file as it is on
  disk, not from the snapshot's copy, and a file that does not parse is left as it is (it lost
  its other keys to `{"list": …}`); the user is told. A vault that cannot be opened is said, in
  Ukrainian when another window or Knotes holds it.
- 2026-11-16: indexing goes on after a failure. A sync, a snapshot or index upkeep that fails is
  said in the window (once for each reason) and tried again with the next change, or 30 s after;
  it ended the indexing thread, and the window showed the last snapshot without a word (empty
  when the first one failed). A folder below the vault that cannot be listed (not allowed, or
  deleted while walked) counts as empty, and a rules file that cannot be read is skipped as one
  that does not parse: either failed the whole vault, before its first snapshot. A vault
  FSEvents does not watch is still indexed (the window says that outside changes are seen when
  it is opened again). When FSEvents says events were lost (`need_rescan`), the batch holds
  `RESCAN` (`""`): the vault is synced, and the window looks at the note open and the embedded
  notes on disk. A file registered but not read yet keeps its date less a nanosecond, not 0:
  it was listed on 01.01.1970.
- 2026-11-16: names sort with Cyrillic in its alphabet's order (the note list by name, folders,
  tags, a table's column, the AI's `list_notes` by name): by code points є, і, ї and ґ (and ё) came
  after я, "Інжир" and "Ґанок" after "Яблуко". `natural_cmp` and `natural_key` moved to
  knotes-core for all of them; the alphabets of Ukrainian and Russian are one order.
- **What the window shows is decided outside it** (2026-11-18). The app's binary grew to hold
  most of the code, its rules tested only by launching it. They move to libraries, the window
  left to ask and to draw: `knotes_app::VaultState` is the window's state (the last snapshot,
  the row selected, its space, the search typed) and says what the sidebar and the note list
  hold; `knotes_app::search` what a search finds (ADR 0015); `knotes_app::ReviewSession` a
  review under way (ADR 0037); `knotes-assistant` the chat's and the vectors' threads (ADR
  0027). A rule is tested where it lives, with no window; the window's tests keep what only
  a window shows.
