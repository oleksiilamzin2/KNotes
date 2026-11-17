# 0038. The toolbar in two rows, the columns under it, and the path bar

Status: accepted, 2026-11-17

## Context

The toolbar held everything in one row of glass groups, each over its column: the sidebar's
switch, the list's name and search, the note's buttons and name, the panel's switch (ADR 0010,
0015, 0019, 0023). The owner showed DEVONthink's window as what he wanted: a row of few
buttons, and under it a plain row where each column has its own controls as text and small
buttons; the columns start under both, the sidebar too; and a path bar under the note, as the
Finder's.

A first reading put the note's name in a header above the note's text; that was not what was
asked and was taken back.

## Decision

- **Two rows.** The toolbar itself keeps what is the window's: the sidebar's toggle at the
  left, the panel's at the right, empty between. Under it a row of its own (a titlebar
  accessory at the bottom, `second_row`), plain on the bar, no glass, each part over its
  column:
  - over the sidebar: what it lists, four small buttons (tabs, folders, tags, cards), the one
    chosen in the accent colour;
  - over the list: its name and its count, the search's magnifying glass and the list's
    options («…»); while the search shows, its field in the place of the name and the count
    (ADR 0015), with «Aa» to ignore case or not;
  - over the note: a new note, back and forward, the document's name (renamed in place, ADR
    0019) and when it was last changed;
  - over the panel: what it shows, three small buttons (the AI, the outline, the links; ADR
    0023, 0026).
  The row's parts follow the split view's columns: they are laid out again when a column is
  resized and when the row's frame changes.
- **The columns start under the toolbar and its row**, the sidebar as the others
  (`setAllowsFullHeightLayout(false)`; at full height it ran up beside the toolbar's buttons).
  The window has no full-size content view: what scrolls is cut at the row's edge, not faded
  under glass (the owner: «щоб текст при прокрутці просто ховався за грань»).
- **The row's ends are grey plates** over the sidebar and over the panel, the columns' own
  grey (the owner's sample, 237 237 238; in the dark 37 37 39): the sidebar's material was
  darker than the panel under it, and in the dark the window's background showed as dark
  plates over lighter columns.
- **A path bar at the bottom of the note column** (`PathBar`, 24 pt): where the open document
  is, from the vault down. The vault is a book alone, no name (it is the whole vault, as in
  the spaces' switcher); then its folders and the document, each with its symbol in the
  accent colour (the Finder's coloured icons stood out). A click on the vault shows all
  notes, on a folder that folder. Hidden with no document open.
- **A new note made from the row's button starts with its name being typed**: the name's
  field empty with a grey «Без назви», the cursor in it, no focus ring (the ring grew in as
  an animation and stuttered while the note was being made).
- **Small things with it**: a little room above the first row of the sidebar, the outline and
  the links, which began right under the row; the columns' first widths are 233, 282 and 317
  points (sidebar, list, panel), the owner's.

## What was learned laying it out

- A hidden view's constraints still hold: a width wished for by a hidden view widened its
  column, and the window with it.
- A width tied to a column must be weaker than the split view holds its columns (250):
  stronger, it kept the column from growing with the window; much weaker, its text collapsed.
- A label that hugs its text can collapse the stack it is in; a button added to a row can
  widen its column by its few points unless it yields first («Aa» widened the list by 2).

## Consequences

- ADR 0016 stands for the scroll views (`QuietScrollView` still turns the pockets' line off),
  but nothing scrolls under the toolbar any more: the glass it kept is the first row's only.
- The list's name and count show in the row; the window's title and subtitle are still set to
  them. The note's name is not a toolbar item. The probe reads the row's controls by their ids
  (`row_control`).
- A deck's review hides the list's column while a card shows (ADR 0037); the row's part over
  the list goes with it.
