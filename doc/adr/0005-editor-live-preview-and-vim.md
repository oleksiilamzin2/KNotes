# 0005. Editor: live preview with a mode switch, and vim

Status: accepted, 2026-11-12

## Context

Notes are plain Markdown files compatible with Obsidian. The editor must render
formulas, images, PDFs and Notion-like tables, and support vim editing (carried over
from Knotes v1 together with git history).

## Decision

- The Markdown file is the only source of truth; the editor never rewrites parts of a
  file the user did not touch.
- Default mode is live preview, as in Obsidian: markup is hidden except on the line
  with the cursor; formulas, images, PDFs and tables render in place.
- A switch offers three modes: live preview, source, reading view.
- vim mode works in all editable modes.
- Block handles (`+`, `⠿`) as in the v1 window act on Markdown blocks, not on a
  separate block model.

## Consequences

- Round-trips are lossless by construction.
- Live preview on TextKit 2 is the hardest UI piece; it gets its own stage and its own
  decisions (incremental parsing, attachments, cursor reveal).
