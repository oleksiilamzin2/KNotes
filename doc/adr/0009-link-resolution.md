# 0009. Link resolution

Status: accepted, 2026-11-12

## Context

ADR 0007 asks for Obsidian-compatible links. Obsidian does not document how
`MetadataCache.getFirstLinkpathDest` picks a file when several share a name; forum
reports say a file in the vault root wins over one in a subfolder, even when the
linking note sits in that subfolder. Links must also survive files that appear later
(broken link becomes valid) and files that go away.

## Decision

Matching:

- Names and paths are compared after NFC normalization and lowercasing. macOS can hand
  out decomposed names (`й` as `и` + combining breve), which would break Ukrainian links.
- A note is linked by its file name without `.md`; other files by their full file name.

Choosing the target (an approximation of Obsidian, kept deterministic):

1. Markdown links `[x](path)`: first relative to the folder of the linking note (`./`,
   `../` applied), as in plain Markdown.
2. Links that contain a folder, and Markdown links: the exact vault path, then any file
   whose path ends with the written path. Otherwise the link is broken.
3. Bare names (`[[Name]]`): the candidate with the shortest path, ties by path order.
4. `[[#Heading]]` points to the note itself; external links never resolve.

Storage and updates:

- `files.name_key` and `links.target_key` hold the folded names; `links.target_id` holds
  the resolved file. Both keys are indexed (re-resolution, backlinks).
- Links of notes written in a batch are resolved before insertion, against all files of
  the index plus the batch.
- When a file appears or disappears, only stored links with that name key are resolved
  again; editing a note does not touch other notes' links.
- A cold build should register all files first (metadata only, 0.3 s for 50k) and then
  write note contents, so no link needs resolving twice.

## Consequences

- Measured on 50k synthetic notes: 96.0% of wiki/Markdown links resolve (the generator
  makes 4% broken), backlinks of a hub with 1292 links in 1.7 ms, cold build 7.8 s with
  two passes on an M5 Pro.
- If Obsidian's real rule for ambiguous names differs, only `Candidates::choose`
  changes; real vaults with duplicate names are the test to run.
- Aliases do not resolve `[[Alias]]` links (Obsidian writes `[[Note|Alias]]`); they are
  used by title search.
