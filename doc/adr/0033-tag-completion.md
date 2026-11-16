# 0033. Tags completed while typed

Status: accepted, 2026-11-16.

## Context

Links complete while typed (ADR 0013); tags did not, so a tag was typed from memory and a
slip made a new one. Obsidian offers the vault's tags after `#`. The owner asked for tags to
complete, and chose (2026-11-16): in the text and in the frontmatter's `tags`; in a space its
tags first, then by how often they are used.

## Decision

- **Where**: after a `#` at a word's start in the text (as the index takes a tag: letters,
  digits, `_`, `-`, `/`), not in inline code or a fenced block, not after `\#`; and in the
  frontmatter's `tags` (or `tag`): a flow list (`tags: [a, b`), a block list (`  - b` below
  `tags:`), or the old words apart (`tags: a b`), quotes and a `#` before the name skipped.
  Right after `#` (nothing typed yet) the most used tags show; a space or `#` (a heading)
  closes the list.
- **Which, in what order**: in a space its own tags first (counted there, as the sidebar's),
  then the rest of the vault's; in each, those starting with what is typed, then those with a
  part (after `/`) starting with it, then those holding it; the most used first. Case is
  ignored. The list shows each with its notes.
- **Choosing**: the same list and keys as links (Up/Down, Return/Tab, a click, Esc closes it
  until the tag is left). In the text the tag goes in with a space after it unless one follows;
  in the frontmatter without `#` and without a space. A tag typed in full with nothing else to
  offer closes the list, so Return makes a new line again.
- **Cost**: the line up to the cursor is read on each key, as for links; the note from its start
  only after a `#` that starts a tag, or in its first 8192 characters when it starts with `---`.
  The tags come from the snapshot (ADR 0030): nothing is asked of the index while typing.
