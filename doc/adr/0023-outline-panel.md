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

## Built (2026-11-13)

- The switch is a toolbar group (a brain and an indented list) over the panel, hidden while
  the panel is collapsed, as the folders/tags one is; the panel's button became the system's
  `sidebar.right`, and View's item "Показати або сховати панель". The choice is `panel` in
  `settings-v2.json`, the AI by default.
- A note's headings come from the Markdown parser the index uses, at their UTF-16 offsets.
  While the outline shows, they are read again 150 ms after typing pauses (typing stays as
  fast); the same headings in the same tree only move, without reloading the rows. A PDF's come
  from its outline through PDFKit (an entry's destination, or its GoTo action's), each with its
  page and how far down the page as shown; the document is let go at once.
- A click scrolls the note so the heading's line is at the top, laid out around it first
  (TextKit only estimates where lines out of view are); the cursor stays where it was. In a
  PDF the heading's place goes to the top.
- As the document scrolls, the heading highlighted is the last at or above the paragraph (or
  the PDF's place) at the top. A heading clicked stays highlighted until the document scrolls
  on from where the click put it: a PDF's last pages cannot reach the top.
- Folds are kept by heading (its title and its parents') while the document stays open, through
  edits that reload the rows; a heading in view folded away highlights the heading it is folded
  under.
- Probe mode clicks a row by selecting it and sending the outline view's own action to its
  target: in the probe's window, which is not key, the outline view tracks no click.
