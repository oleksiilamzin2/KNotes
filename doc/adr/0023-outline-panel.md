# 0023. The right panel: AI or the document's outline

Status: accepted, 2026-11-13

## Context

The right column (the inspector) holds the chat's stub: a title and the question field; the
chat is a later stage. The roadmap had a stage for heading marks at the note's right edge with
a floating panel (as in Notion). The owner asked for the right panel to show either the AI or
the outline of the document open, a note or a PDF alike, and answered a question round
(2026-11-13).

## Decision

- A switch "ШІ | Зміст" in the toolbar over the right panel chooses what the panel shows, as
  "Теки | Теги" does over the sidebar; the button that shows and hides the panel stays. The
  choice is kept in the app's settings.
- The outline lists the headings of the note open (its Markdown headings) or the PDF open
  (the outline the file carries, its bookmarks). A PDF without one shows "У цьому PDF немає
  змісту"; no headings are guessed from the text (that would read every page, against keeping
  only the pages around the one shown, ADR 0022).
- A click on a heading scrolls the note or the PDF to it. The heading of the part in view is
  highlighted as the document scrolls. Headings with headings under them fold and unfold.
- The panel replaces the roadmap's heading marks at the note's edge (owner's choice): the
  headings have one place.

## Consequences

- Headings are read again as the note is edited; the outline follows the text.
- Most scanned and many generated PDFs carry no outline and show none.
