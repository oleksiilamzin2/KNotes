# 0012. TextKit 2 spike: build the editor on it

Status: accepted, 2026-11-12

## Context

ADR 0011 asked for a spike before building the editor on TextKit 2: hide Markdown markup
outside the cursor line, handle a 300 KB note at 120 fps. Spike: the note column's
`NSTextView` (TextKit 2 confirmed: `textLayoutManager` present, `layoutManager` never
touched) styled from `knotes_editor::highlight`, markup hidden by attributes.

## Findings (M5 Pro, 300 KB synthetic note; M1 estimate is 2x)

| Operation | M5 | M1 estimate | Budget |
|---|---|---|---|
| Open and style the note | 10.6 ms | 21 ms | 16 ms for 100 KB notes (~3.5 ms at 100 KB) |
| Cursor to next paragraph (restyle, scroll, layout): p50 / p95 / max | 0.15 / 1.37 / 6.0 ms | max ~12 ms | 8.33 ms frame |
| Scroll 40 pt steps: p95 / max | 0.52 / 1.08 ms | ~2 ms | 8.33 ms frame |
| Highlight and restyle the whole note | 2.7 ms | ~5.5 ms | 4 ms main-thread work per frame |

Hiding by attributes (near-zero font size and clear colour) leaves no visible gaps, keeps
the file text untouched, and the caret only ever walks hidden characters on other lines,
whose markup is revealed as soon as the caret arrives.

## Decision

- Build the editor on TextKit 2 with attribute-based hiding.
- Edits restyle only the changed paragraphs (plus runs that span them, such as code
  blocks), never the whole note on every keystroke: the full restyle misses the per-frame
  budget on an M1 for 300 KB notes.
- Rendering still to do: strikethrough line, list bullets and task boxes, quote bars,
  hidden code fences, and attachments for formulas, images, PDFs and tables.

## Consequences

- Native text behaviour stays: IME, Ukrainian spell check, dictation, undo.
- Cursor moves can occasionally cost ~12 ms on an M1 (one dropped 120 Hz frame); watch it
  with the editor bench (`KNOTES_PROBE_ACTION=editor-bench`).

## Measured with editing, 2026-11-12

Edits restyle the region from `knotes_editor::changed_region`: the inserted text plus the
runs and markup between the common prefix and suffix of the old highlight (moved by the
edit) and the new one. The highlight is computed on a Rust copy of the text, updated by
each edit, because copying a 300 KB `NSString` alone took about 1 ms.

| Operation, 300 KB note, M5 Pro | p50 / p95 / max | M1 estimate | Budget |
|---|---|---|---|
| Keystroke: highlight, region, restyle, layout, display | 2.0 / 2.2 / 6.3 ms | ~4 ms p50 | 4 ms of own work per frame, layout and drawing aside |

Of the keystroke, about 1 ms is own work (highlight 0.6 ms, UTF-16 offsets 0.35 ms); the
rest is layout and display. A fence typed near the top restyles everything below it, like
a full restyle (~4.5 ms): rare, at most a dropped frame on an M1.
