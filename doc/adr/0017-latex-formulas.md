# 0017. LaTeX formulas in the live preview

Status: accepted, 2026-11-12; typesetter changed to RaTeX 2026-11-13

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

RaTeX (KaTeX's layout and fonts in pure Rust, MIT) was missed by the spike; tried on
2026-11-13 at the owner's request: all 87 fixture formulas plus `\middle` and mhchem's `\ce`
typeset, p50 0.06–0.11 ms and max 0.7 ms per formula (release, M5 Pro), KaTeX's Computer
Modern look.

## Decision

- Typeset with RaTeX (`ratex-parser`, `ratex-layout`, `ratex-render` with embedded KaTeX
  fonts; pure Rust, no JavaScript or web view). It replaced `latex-rust` the day after: ten
  times faster, closer to Obsidian's look, `\middle` and `\ce` work.
- Letters the KaTeX fonts lack (Cyrillic in `\text{…}`) are drawn by RaTeX in a system font;
  their widths are measured in that font and handed to the layout (without them each is half
  an em wide: gaps after `і`, overlaps after `ш`). The app points RaTeX at Times New Roman
  (`RATEX_UNICODE_FONT`, set in `main`), a serif face as MathJax in Obsidian uses for unknown
  letters; scripts it lacks fall back to Arial Unicode.
- As in Obsidian (owner's choice): outside the cursor's formula, `$…$` and `$$…$$` show
  typeset, display formulas centred; with the cursor in a formula its source shows, and a
  display formula also shows a live preview below its source. A click on a formula puts the
  cursor in its source.
- The note text stays the source: the formula's characters are hidden (as hidden markup),
  their width reserved with kerning and their height with the line height, and the image is
  drawn at that place by the paragraph's own TextKit 2 layout fragment (a subclass of
  `NSTextLayoutFragment` the layout manager's delegate returns for paragraphs with formulas).
  Clicks reach the text.
- Images are cached by source, style and colour; invalid LaTeX shows its source marked as
  an error.

## Consequences

- Formulas look like KaTeX's, close to Obsidian's MathJax (both Computer Modern); macros such
  as `\newcommand` untested.
- RaTeX is young (0.1.x, thin API docs); the typesetter stays behind `render_formula`, so a
  change of library touches one function. It writes a line to stderr when it loads the
  fallback font.
- Images follow layout by construction: TextKit asks for a new fragment whenever a paragraph's
  attributes change, and every change of what a formula shows (cursor in or out, an edit)
  restyles it. A change of colour scheme (light/dark) typesets the formulas again in the new
  ink.

## Built (2026-11-12)

- First built with `CALayer`s over the text view, placed on the next turn of the run loop
  after a restyle and again on scroll, for the visible formulas only. The images jumped while
  scrolling: TextKit 2 lays out and re-estimates paragraph heights as the viewport moves,
  and the layers followed a run loop later. Drawing in the layout fragment replaced them
  (and the layer pool, scroll observer and deferred placement).
- 300 KB note with ~1000 formulas (M5 Pro, release): placing every layer took 480–580 ms (it
  laid out the whole note), the visible ones ~1.8 ms. With fragments (611 KB, ~1300
  formulas): scroll step p50 0.35 ms, p95 0.48 ms, keystroke p50 2.2 ms, the same as with
  layers, and no work left after the step.
- Spelling marks are kept out of formulas: their source is not words, and on hidden glyphs
  the marks showed as stray dots.
- A fixture note (`lib/knotes-editor/tests/fixtures/formulas.md`, 87 formulas of the kinds
  Obsidian notes use) typesets in full; the one gap found with latex-rust, `\middle`, typesets
  with RaTeX and joined the fixture.
