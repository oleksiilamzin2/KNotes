# 0002. Native UI with AppKit through objc2

Status: accepted, 2026-11-12

## Context

The UI must look native on macOS 27 (Liquid Glass sidebar, toolbar, controls), run at
120 fps on an M1 and keep the CPU idle when nothing changes. The whole app is Rust.
Options considered:

| Option | Native look | 120 fps | Text input (spell check, dictation, IME) | Testability |
|---|---|---|---|---|
| AppKit through `objc2` | real | yes, system compositor | TextKit 2, full | needs a test channel |
| GPUI (Zed) | imitation, no Liquid Glass | yes | custom, weaker | headless tests |
| Swift UI + Rust core over FFI | real | yes | full | good, but UI is not Rust |

## Decision

AppKit through `objc2` / `objc2-app-kit`:

- Window, sidebar, toolbar and split view: `NSSplitViewController` with native sidebar
  material; four columns: folders | notes | note | chat.
- Folder and note lists: view-based `NSOutlineView` / `NSTableView` with cell reuse, so
  50k rows cost the same as 50.
- Note editor: TextKit 2 (`NSTextView` with `NSTextContentStorage`), attachments with
  view providers for formulas, images, PDFs and tables.
- PDF: PDFKit.
- Graph and canvas: custom drawing with `wgpu` (WGSL compiled at run time, so no Xcode
  `metal` tool is needed) inside an `NSView` backed by `CAMetalLayer`.
- All I/O, parsing, indexing and search run off the main thread; the main thread only
  applies finished results.

Reference look: the window of Knotes v1 (sidebar with folder counts, Apple Notes style
list "title, time, snippet", block handles `+ ⠿`, chat column on the right).

## Consequences

- Native behaviour for free: accessibility, spell check in Ukrainian, dictation,
  Writing Tools, system scrolling.
- A lot of `unsafe` glue; it is kept inside the UI crate behind small safe wrappers.
- UI logic that matters is kept in plain Rust view models, tested without AppKit; the
  AppKit layer is checked through a test command channel and window snapshots.
- macOS only. Accepted.
- Minimum macOS version is still open (proposal: 26).
