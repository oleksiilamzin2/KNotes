# 0026. The right panel's connections: backlinks, unlinked mentions, outgoing links

Status: accepted, 2026-11-14

## Context

The index keeps every link between vault files (ADR 0009) and uses them to update links on a
rename (ADR 0019), but the window shows none of them: nothing tells which notes link to the
document open. The right panel switches between the AI and the outline (ADR 0023). The owner
asked for backlinks and answered a question round (2026-11-14).

## Decision

- The right panel gets a third segment, "Зв'язки" ("ШІ | Зміст | Зв'язки"), for the document
  open, a note or a PDF (owner's choice over a list under the note's text).
- It lists three sections (owner's choices):
  - links to the document from other notes, by note, each with the line around the link;
  - unlinked mentions: notes whose text names the document (its name as the toolbar shows it)
    without linking to it, each with the line, and a button that makes the mention a link;
  - the document's own outgoing links, those to notes not there yet dimmed.
- A click on a line opens its note in the tab shown at that place, the link (or mention)
  selected and pointed at (owner's choice over opening at the top); a click on a note opens it;
  a click on an outgoing link follows it.

## Consequences

- Unlinked mentions are found through the full-text index, then checked in the notes' text:
  as fresh as the index.
