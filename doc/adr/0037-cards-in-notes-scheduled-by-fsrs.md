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

## Built (2026-11-17): the index

- The index (schema 14) keeps a note's cards (`cards`: deck, id, where in the text), found by
  `scan_cards` as the note is indexed (Markdown only; a note without `#@` is not scanned); the
  reviews taken from the logs (`reviews`), how much of each log was read (`review_logs`), and
  each reviewed card's schedule (`card_states`). The tables are a cache as the rest: a rebuilt
  index reads the logs again from their start.
- A review is a line of a log, `when<TAB>card id<TAB>grade` (`review_line`, `parse_review`).
  `SqliteIndex::take_reviews` takes a log's text from where it was read to, keeps its whole
  lines' reviews (a line still being written waits), and works out again the schedule of each
  card they are of from all its reviews in time's order (`schedule`), whatever log they came
  from: a second Mac's reviews arriving late fall into place. A card graded Again is due in ten
  minutes, any other after its interval.
- `CardIndex` (knotes-core): `deck_counts` gives each deck's own cards, those due and those
  never reviewed; `due_cards` a deck's and its sub-decks' cards to review, the longest due
  first, then the new ones.
- Measured (M5 Pro, release) on 10,000 notes with 100,000 cards and a million reviews: the
  cards add nothing one can see to indexing (447 ms for the notes, cards and all); the million
  reviews are taken in 0.58 s (a rebuilt index's cost, once); one review in 0.12 ms; the decks'
  counts (200 decks, 60,000 due) in 34 ms; a deck's next 200 cards in 1.3 ms, the whole vault's
  in 42 ms.
- To come before the window: the logs' files in the vault's `.knotes` folder (a log a Mac), read
  by the indexer as they grow.

## Built (2026-11-17): the logs' files and the sidebar's decks

- The review logs are files in the vault, `.knotes/reviews/<name>.tsv`, a file a Mac, only ever
  appended to (`FsVault::append_review`, `review_logs`, `read_review_log`): two Macs syncing
  the vault never write one file. The session takes in what each holds past where it was read
  to at every sync (`take_reviews`); a log shorter than what was read (replaced) is read again
  whole.
- A snapshot carries the decks (`Snapshot::decks`, each deck's own cards counted, those due at
  the snapshot's time and those never reviewed). The count a deck shows is its cards to review,
  its sub-decks' with them (`deck_tree`, the tags' tree reused).
- The sidebar has a fourth mode, «Картки», a fourth small button over it (after tabs, folders
  and tags): the decks as a tree. A deck chosen lists the notes its cards are written in.
- Left: what is due is counted at the snapshot's time, so a card that falls due while nothing
  changes is counted with the next change; the review itself and making a card from a note are
  to come.

## Built (2026-11-17): a card made from a note, and the review

- **Edit → «Зробити картку»** (⇧⌘K, and first in the note's context menu with something
  selected): what is selected becomes a card where it is. Its deck is asked (the decks there
  are to choose from, the last one used offered) and its question, the selection being the
  answer; a selection that asks already (`question::answer`, a cloze) is asked only its deck.
  The card gets a paragraph of its own and its id at once (`make_card`, `card_id`); it is
  written as typed (Undo takes it back) and saved.
- **A deck chosen in the sidebar shows its review** in the note column: how many cards wait
  and «Почати». Then a card's question; «Показати відповідь» (Space); its answer and four
  grades (keys 1 to 4), each saying when the card would come back («Добре · 3 д»), worked out
  from the card's state (`CardIndex::card_state`); «Відкрити нотатку» goes to the card in its
  note. A review takes the 200 cards longest due (then the new ones); a card graded Again comes
  back at its end. A document opened, or another list chosen, ends it.
- **A grade** writes a line to this Mac's log in the vault (`AppSettings::device_name`, made
  once: `mac-a1b2c3`); a card with no id is given one in its note first (`give_id`; what is
  typed and not saved is written before, not written over). The card is read from its note as
  it is now (by its id, or where it was): one gone or changed since is passed over.
- Left: cards show as plain text, in the review and in the note (no live preview of a card, no
  Markdown in the review); no limit of new cards a day; the review covers the note open, whose
  name stays in the toolbar's row.
