# 0007. Obsidian compatibility

Status: accepted, 2026-11-12

## Context

Users should be able to open an existing Obsidian vault without changes, and the
owner's real vault doubles as test data.

## Decision

Knotes reads and writes Obsidian-flavoured Markdown:

- wikilinks `[[note]]`, `[[folder/note]]`, `[[note#Heading]]`, `[[note#^block-id]]`,
  `[[note|alias]]` and embeds `![[...]]`;
- Markdown links `[text](url)`; internal ones point to vault files, external ones have
  a URL scheme;
- tags `#tag` and nested `#area/sub`, tags in YAML frontmatter;
- YAML frontmatter properties, including `tags` and `aliases`;
- link resolution as in Obsidian (exact path first, then by file name);
- block ids `^id`, callouts `> [!type]`, math `$…$` and `$$…$$`, GFM tables;
- canvases in the open JSON Canvas format (`.canvas`).

The indexer parses Markdown with `pulldown-cmark` 0.13, which supports wikilinks, math
and YAML metadata blocks natively; tags, block ids and Obsidian-only syntax are
extracted on top of its event stream. The editor may need an incremental parser; that
is decided at the editor stage.

## Consequences

- Corner cases follow Obsidian's behaviour, checked with tests.
- Ignored folders by default: `.obsidian`, `.trash`, `.git`.
