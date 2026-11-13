# 0025. Syntax highlighting in code blocks

Status: accepted, 2026-11-13

## Context

Fenced code blocks show in the live preview in a monospaced font on a background, their fences
folded (ADR 0018); the code is one colour, and the language label and copy button ADR 0018
left for later are missing. The owner asked for highlighting and answered a question round
(2026-11-13).

## Decision

- Code in a fenced block with a language (```` ```rust ````) is highlighted with syntect
  (Sublime Text / TextMate grammars), with the extended set of grammars bat ships (the
  `two-face` crate: TypeScript, Swift, TOML, Kotlin, Dockerfile and more beside syntect's own)
  (owner's choice over syntect's set alone and over tree-sitter, whose grammars are compiled in
  one by one).
- Colours are Xcode's default ones, light and dark (owner's choice over GitHub's and One's).
- A block without a language stays one colour (owner's choice over guessing it).
- A block shows its language small at its top right corner, a copy button when the pointer is
  over it (copying the code without its fences), and dim line numbers at its left (owner's
  choices).

## Consequences

- The app grows by a few megabytes of grammars; they are read once, in the background.
- Typing in a block highlights that block again, not the note.

## Built (2026-11-13)

- `knotes-editor` parses code with syntect (Oniguruma regexes) and `two-face`'s grammars, read
  on a thread of their own at launch. Each scope is sorted into a kind (comment, string,
  number, keyword, type, function, constant, preprocessor, attribute; operators and
  punctuation plain), and the view colours kinds with dynamic colours, Xcode's light and dark
  ones, so a change of appearance needs no new highlighting. Keywords are semibold, as Xcode
  sets them.
- A block keeps a highlighter that remembers each line's text and the parser's state at its
  start and end: an edit parses again only the lines it changes, until the state after them is
  what it was (opening a comment parses the lines after it). Typing in a 500-line Rust block
  (M5 Pro, release): p50 0.95 ms a key, against 0.8 ms without a language and 7.6 ms when every
  key highlighted the whole block. The block's first highlighting costs about 16 ms.
- The block's paragraphs are indented by a gutter as wide as its last line number; the
  numbers are drawn by each code line's fragment, dim, right-aligned before the code. The first
  code line's fragment draws the language (only while the fences are folded: shown, they say
  it) and, with the pointer over the block, a copy button; a click on it copies the code
  between the fences without its last line break.
