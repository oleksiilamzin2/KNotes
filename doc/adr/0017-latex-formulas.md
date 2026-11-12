# 0017. LaTeX formulas in the live preview

Status: accepted, 2026-11-12

## Context

Notes write formulas as Obsidian does: `$…$` inline, `$$…$$` display (MathJax there). The
live preview (ADR 0005, 0011) should show them typeset. A spike rendered eight formulas
(fractions, sums, integrals, matrices, aligned, cases with Cyrillic `\text`, `\mathbb`,
`\vec`) with two Rust options:

| | latex-rust 3.0 | MathJax via mathjax-svg-rs 0.4 (Boa) |
|---|---|---|
| Rendered | 8 of 8 | 6 of 8 (fails on Cyrillic `\text`, `\mathbb`) |
| Time per formula (M5) | 0.5–1.4 ms | 8–17 ms, first 214 ms |
| Font | STIX Two Math | Computer Modern |

## Decision

- Typeset with `latex-rust` (pure Rust, no JavaScript or web view, TeX layout, MIT/Apache).
- As in Obsidian (owner's choice): outside the cursor's formula, `$…$` and `$$…$$` show
  typeset, display formulas centred; with the cursor in a formula its source shows, and a
  display formula also shows a live preview below its source. A click on a formula puts the
  cursor in its source.
- The note text stays the source: the formula's characters are hidden (as hidden markup),
  their width reserved with kerning and their height with the line height, and the image is
  drawn as a layer over the text view at that place (layers take no clicks).
- Images are cached by source, style and colour; invalid LaTeX shows its source marked as
  an error.

## Consequences

- Formulas may look a little different from Obsidian's (STIX Two Math instead of Computer
  Modern; macros such as `\newcommand` untested).
- Layers must follow layout: they are placed after every restyle and resize.

## Built (2026-11-12)

- Layers are placed on the next turn of the run loop after a restyle (placing at once read
  frames of paragraphs not laid out again yet: images landed a line off), only for the
  formulas TextKit laid out for the visible part, and again when the note scrolls.
- 300 KB note with ~1000 formulas (M5 Pro, release): placing every formula took 480–580 ms
  (it laid out the whole note); placing the visible ones takes ~1.8 ms. Keystroke p50
  1.6 ms, unchanged.
- Spelling marks are kept out of formulas: their source is not words, and on hidden glyphs
  the marks showed as stray dots.
