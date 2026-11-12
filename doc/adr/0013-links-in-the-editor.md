# 0013. Links in the editor: click, missing notes, completion

Status: accepted, 2026-11-12

## Context

ADR 0011 asks for clickable wikilinks with completion in the live preview. The index
resolves stored links (ADR 0009), but the editor needs the same answer for text that is
not saved or indexed yet, on every restyle and click. The owner chose the behaviour.

## Decision

Click, as in Obsidian:

- Where the markup is hidden (any line but the cursor's), a plain click on a link follows
  it. In the cursor line a plain click places the cursor; Cmd+click follows the link
  anywhere.
- `[[Note]]` and `[x](note.md)` open the note in the same window, through the history
  (Back returns). `#Heading` scrolls to that heading, `^id` to that block. External URLs
  open in the default app.
- Links to missing notes are drawn dimmer. A click creates the note and opens it: a link
  with a folder (`[[Folder/Name]]`) at that path, a bare name in the new notes folder:
  `Вхідні` by default, set by `{"newNotes": {"folder": "..."}}` in the vault root's
  `.knotes`. The same folder takes Cmd+N notes when no folder is selected.

Resolution in memory:

- A `LinkTargets` value holds every vault path by folded name and resolves with the same
  rules as the index (one implementation of the choice, ADR 0009). The session builds it
  with each snapshot, in the background, and reuses it while the set of files is the same.

Completion:

- Typing `[[` opens a floating list under the cursor: fuzzy title search (the in-memory
  title index), each row with the title and folder. After `#` it lists the target's
  headings. Up/Down choose, Return/Tab accept, Esc closes; the text view keeps the focus.
- The inserted link is the shortest that resolves to the chosen file from the current note:
  the bare name when it does, otherwise the full vault path without `.md` (Obsidian's
  "shortest path when possible"). The closing `]]` is added when missing.

## Consequences

- Link colours in the open note follow file creation and deletion, at the cost of a
  restyle of the open note when the file set changes.
- Block (`^id`) completion waits until block ids are exposed by the index.
- Measured at 50k notes (M5 Pro, release): the title index and link targets add about
  14 MB (resident 245 → 259 MB with a warm index). Separately found: after a cold index
  the physical footprint stayed at 432 MB until the app restarted; fixed by ADR 0014.
