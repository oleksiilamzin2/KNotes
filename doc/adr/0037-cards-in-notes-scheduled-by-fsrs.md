# 0037. Cards in notes, in decks, scheduled by FSRS

Status: accepted in part, 2026-11-17 (the core is built; the index, the review and the window
are to come).

## Context

The owner asked for spaced-repetition cards: scheduled by FSRS, each tied to the note it comes
from and made right from that note, in decks that nest, for 10,000 to 100,000 cards in a vault.
It is the first new feature since the owner's goal became to polish what exists.

## Decision

- **A card is text in its note.** A paragraph that starts with its deck, `#@deck/sub`, and reads
  one of three ways: `question::answer` on a line; the question, a line `?`, the answer, over
  several lines; or a text with `==words==` left out (a cloze, all of them asked at once). It
  stays the note's: edited there, moved with it, found by search. A list item may be a card.
  Code blocks and the frontmatter hold none.
- **Its deck starts it**, a path whose parts nest: `#@мови/англійська`. `#@` is the owner's
  choice: neither Knotes nor Obsidian reads it as a tag, so decks stay out of the tags. A deck
  is there while a card names it; nothing else makes or keeps one.
- **Its id ends it**, `^c-a1b2c3`, as an Obsidian block id: written once, it ties the card to
  its reviews through any edit. A card without one has none until it is given one (when it is
  made from the window, or first reviewed).
- **Reviews are not written into notes.** With 100,000 cards each review would rewrite a note
  (and conflict in iCloud or git). To come: a log of reviews in the vault's `.knotes` folder,
  appended to, going with the vault; and each card's schedule worked out from it in the index
  (outside the vault, rebuilt at will, as the rest of the index).
- **FSRS-5 with its default weights**, cards due when their recall has fallen to 90%; intervals
  in whole days, one at least. No fitting of the weights to the owner's reviews, no fuzz of the
  intervals yet.
- To come in the window: «Зробити картку» from a selection in a note (its deck asked, with the
  decks there are to choose from); a fourth mode of the sidebar, the decks as a tree with the
  cards due in each; a card shown as one in the live preview; the review in the note column
  (the question, the answer, four grades, the note it comes from).

## Built (2026-11-17): the core

- `knotes-cards`, with no files and no clock: `scan_cards` gives a text's cards (deck, id, kind,
  front, back, where it is in the text); `Fsrs::review` gives the memory (stability, difficulty)
  after a review from the memory before, the days since and the grade, `Fsrs::interval` the
  days until a card is due, `Fsrs::recall` the chance of recalling it.
- Measured (M5 Pro, release): 100,000 cards in 10,000 notes (19 MB of text) scanned in 93 ms;
  200,000 reviews worked out in 2.5 ms.
