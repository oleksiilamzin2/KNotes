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
