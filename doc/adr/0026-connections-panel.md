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

## Built (2026-11-14)

- `knotes-app` finds a note's mentions of the document: its links resolving to it (embeds
  included), its name as a whole word outside links, ignoring case, and its own links grouped
  by target (web links left out). Each comes with its line, cut around it past 70 characters.
- The notes linking to the document (the index's backlinks) and naming it (a full-text search
  of its name) are gathered on a thread of their own, with a reader of the index of its own,
  each note read and parsed there for the lines; a newer gathering calls an older one off. The
  main thread only lists them, and the document's own links, from the editor's text. Another
  document clears the list at once; the same one keeps it until the new one comes, and a list
  equal to the one shown changes nothing (rows keep their folds and scroll). Typing changes
  only the outgoing links, worked out on the main thread as it pauses; a snapshot (links in
  other notes come and go) gathers again.
- The note most linked in the 50k vault (1291 links from 1232 notes; M5 Pro, release): the
  first version gathered and listed on the main thread, 213 ms, on every snapshot and typing
  pause. Now the main thread spends 14 µs asking and 18 ms listing (27 ms with drawing) once
  the 163 ms gathering comes back; a note with a few links, about 7 ms.
- Listing: a section lists its first 50 notes and a "Ще N…" row for the rest (unfolding 1232
  notes took 21 ms); past 20 notes they are folded, their lines made when unfolded; rows'
  heights are worked out (one line, or a mention's text wrapped, three lines at most), not laid
  out row by row; cells are laid out by frame. What is left is AppKit making the rows in view,
  about 0.4 ms each.
- The list is an outline view in the plain style: in the source-list style AppKit drew its
  text with the field's own font, dropping a line's bold link. Lines wrap to three lines.
- "Make a link" writes the note with `[[Name]]`, or `[[Name|words]]` when the words differ, the
  link's target as short as it resolves; a note changed since (the words not there any more)
  is left alone.
