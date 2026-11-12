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
