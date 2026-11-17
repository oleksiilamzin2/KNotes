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

## Built (2026-11-17): a tab, braces, links

- **A deck's review is a tab of its own** (`deck://path` among the tabs' paths, which no vault
  path can be), opened beside the one shown: the note open keeps its tab. A note opened while
  the review shows opens beside it. The tab is kept for the next launch while its deck has
  cards. The list's column hides while a card shows.
- **A card in braces holds several paragraphs** (owner's syntax): a line `#@deck{`, the
  question, a line `?`, the answer, a line `}`; blank lines inside do not end it. Its id goes
  after the brace (`} ^c-a1b2c3`). «Зробити картку» writes one so when what is selected has a
  blank line.
- **Links in a card's text work in the review**: `[[note]]`, `[[note|shown]]` and
  `[shown](address)` show as in the note (`text_runs`) and are clicked: a note's opens it in a
  tab beside the review's, the web's in the browser. The question and the answer are one text
  that scrolls, the buttons under it. In the note itself a card's links were links already.

## Built (2026-11-17): the chat and MCP see the cards

- The vault's tools (ADR 0027) knew nothing of cards: a card was a line of its note. Two more,
  reading only: **`list_decks`** (every deck with the cards in it and under it, as the sidebar
  counts: all, due now, never reviewed; then the vault's) and **`list_cards`** (a deck's with
  those under it, a note's, or all: question, answer, note, `new`/`due`/`later`, and for a
  reviewed one when it is due and was last reviewed, its reviews, how many forgotten, FSRS's
  stability and difficulty; by state, sorted as the review asks them, by the most forgotten, or
  by note). `list_decks`'s description tells the syntax; the chat's instructions and the MCP
  server's name the two.
- One query serves both: `CardIndex::card_rows(deck, note)`. The cards of a note kept from the
  AI (ADR 0036) are left out there and from the counts, so the counts are made in the tool, not
  by `deck_counts`: every card's row is read (100 000: one indexed join, no text). Questions
  and answers are read from the indexed text of the page's notes only.
- Not done: the agent grades nothing and makes no card (the tools read only). An MCP server
  beside a closed Knotes sees the reviews last taken into the index.

## Built (2026-11-17): a deck's settings

- **Two settings a deck** (owner asked for what Anki has; these first): the chance of recall
  its cards come back at (0.9 unless set, 0.7 to 0.97) and its new cards a day (20 unless set,
  Anki's; before, every new card was asked at once). A deck that sets none takes the nearest
  deck's above (`DeckSettings`). Set from the deck's menu in the sidebar («Налаштування
  колоди…»): two fields, empty for «as above», whose placeholders say what above gives.
- **Kept in the vault**, `.knotes/decks.json` (`{"мови": {"retention": 0.85, "new_a_day":
  10}}`): they go with the notes to the user's other Macs, as the review logs do. Keys Knotes
  does not know stay when it writes the file.
- **The chance of recall moves when a card is due**, not what is remembered of it: the session
  schedules a reviewed card by its deck's (`put_schedule`), and when the file's text is not the
  one the index scheduled with (kept in `deck_settings`), every reviewed card again
  (`take_deck_settings`). A card moved to a deck with another chance keeps its day until its
  next review or the next change of settings.
- **The day's new cards**: `card_states.first` is a card's first review; a deck with those
  under it has taken today as many new cards as were first reviewed since local midnight
  (`CardIndex::introduced`), and asks what is left of its own number (`due_cards`'s `new`); the
  sidebar counts the same (`deck_tree`). A deck's number bounds the deck reviewed, with those
  under it; a deck above does not bound a review started in one below (Anki does).
- Index schema 15 (`first`, `deck_settings`): built again from the notes and the logs.
- Not done: learning steps, the longest interval, reviews a day, burying and suspending;
  weights fitted to the user's reviews; `list_decks` does not give the settings.

## Built (2026-11-17): a deck's statistics

- A deck chosen and not started shows, small under what waits in it, how it stands with the
  decks under it (`CardIndex::deck_stats`): its cards new, young and mature (due 21 days or
  more after the last review: Anki's measure); today's reviews and the last 30 days' with the
  share recalled (not graded Again), a bar a day; and what comes due tomorrow, in 7 days and
  in 30, a bar a day; and those days' grades.
- **Drawn as charts** (owner: «справжні графіки як у Anki»; the first version was text with
  block characters): one view (`DeckCharts`) in place of the card's text, scrolled when taller
  than the column. The cards as one bar in three parts with its legend; what comes due and
  the reviews (the forgotten over the recalled) as a bar a day on one scale each, three lines
  with their numbers; the grades as four rows. The bar under the pointer says its day and its
  number where the chart's sums stand. Three hues apart for every eye, their own steps in the
  dark look; text in the labels' colours. The same in words is the view's VoiceOver label
  (and the probe's `deck-stats:` line). No cumulative line over the forecast as Anki draws:
  it needs a second scale.
- What comes due counts the reviewed cards as scheduled now: no new cards to come, no
  reviews those days will add. Days are the Mac's, from local midnight.
- Not done: a card's ease and intervals spread, the hours reviewed at, time spent (not
  logged), the whole vault's statistics, statistics for the agent.

## Built (2026-11-17): a review ahead of the schedule

- Beside «Почати», «Повторити наперед (N)» (owner asked for a button to review a deck when
  nothing is due; of three ways offered he chose the one that logs): after what is due, the
  deck's reviewed cards not due yet, the soonest due first (`CardIndex::cards_ahead`), up to
  the review's 200. Their grades are logged and schedule them as any review's (FSRS counts
  the days since the last one, so an early Good lengthens the interval little). Shown while
  the deck has such cards; new cards past the day's number are not asked.

## Checked (2026-11-17): FSRS against the reference

- The formulas were written from memory. Compared now, one by one, with the reference
  implementation's source (py-fsrs 4.1.2, FSRS-5; read, not run: installing it was not
  allowed here): the weights, the curve, the interval, the first memory, the difficulty (its
  linear damping and its pull to an easy card's), the stability after a recall and the same
  day were the same. Two were not, and are now: **the days since the last review are whole**
  (a day and a half is a day: the weights were fitted so; Knotes counted the part), and **a
  forgotten card's stability is bounded** by its stability over e^(w17·w18), not by its
  stability alone. A first review's stability is at least 0.1, as the reference's.
- The reference's own tests' numbers are Knotes' tests now (`tests/fsrs.rs`): Good six times,
  Again twice, Good five times give 4, 14, 44, 125, 328 and 7, 16, 34, 71, 142 days; Again and
  Good six times give a stability of 49 days and a difficulty of 7.0866. Both passed before
  the two changes too (neither bites there); each change has a test of its own.
- Index schema 16: the cards are scheduled again from the logs by the corrected formulas.
- Still different on purpose: no learning steps (a new card graded Good comes back in days,
  not in ten minutes), Again comes back in ten minutes whatever the stability, no fuzz.

## Built (2026-11-17): the agent sees the settings and the statistics

- `list_decks` counted every new card as waiting, the sidebar only what the deck's day still
  takes: the agent's number was not the app's. It gives now, a deck: its cards new, young and
  mature, the due, what the app asks now (`to_review`), its new cards a day with how many today
  has taken, and its chance of recall (read from `.knotes/decks.json`, as the app reads it).
  With `deck`, that deck's statistics: what comes due tomorrow, in 7 and in 30 days, and the
  reviews of today and of 30 days with the share recalled and the grades.
- The reviews are counted by the index over every card of the deck: when a note kept from the
  AI holds one, they are left out and said so, the counts made without that note's cards.
- A day begins at the user's midnight: the app tells the tools its clock's offset, the MCP
  server asks `date +%z` (UTC's midnight if that fails).

## Built (2026-11-17): decks follow the spaces

- Owner: «привяжи колоди до просторів відповідно до файлів». In a space (ADR 0030) the decks
  are those its notes' cards are in, and each is the space's part of it: the sidebar's counts,
  a review's cards (the due, the day's new, the ones ahead), the statistics with their reviews,
  and the deck's notes in the list. A deck is no one space's: «мови» with cards in two spaces
  shows in both, each with its own. The whole vault shows all.
- The index knows no spaces. In a space the window reads every card's row and keeps those the
  space holds (`space_rows`), then counts and queues them as the index does for all
  (`knotes_app::deck_rows`); `deck_stats` takes which notes count. The whole vault is still
  counted by SQLite. A space of a vault with 100 000 cards reads them all at each snapshot.
- A deck's settings stay the deck's, the same in every space; its day's new cards are counted
  in the space reviewed (two spaces' parts of one deck each take the deck's number).
- With `deck_stats` taking the notes that count, `list_decks` gives a deck's reviews without
  those of notes kept from the AI, where it gave none.

## Measured (2026-11-17): 100 000 cards

- The goal is 10 000 to 100 000 cards; nothing had been measured. `knotes-bench` measures the
  cards now when the vault has any. On a generated vault (10 000 notes, 100 000 cards in 296
  nested decks, 444 000 reviews in two logs; this Mac, release build):

  | Operation | Time |
  |---|---:|
  | Take the review logs in (all, as after an index rebuild) | 0.66 s |
  | Schedule every reviewed card again (settings changed) | 0.44 s |
  | Every deck's counts (each snapshot, off the main thread) | 46 ms |
  | Today's new cards by deck | 40 µs (was 51 ms) |
  | The largest deck (12 600 cards): cards due / ahead / its notes | 8 to 9 ms |
  | The largest deck's statistics | 23 ms |
  | In a space (56 000 cards): its decks' counts | 44 ms |
  | In a space: the largest deck's cards due / its statistics | 40 ms / 23 ms |

- Two were felt, both on the main thread at every snapshot (a snapshot follows every note
  saved). **Today's new cards** read every card: the first reviews are indexed now
  (`card_states_by_first`, schema 17) and the query starts from them. **The decks are counted
  again only when they may have changed** (`DecksCounted`: the vault's own counts, its files,
  the spaces' folders, the window's space, the settings' text, the day): a note without cards
  saved costs nothing, in a space too. Left: a card moved between two spaces' notes with every
  deck's count the same shows at the next change.
- Left as they are: the counts of every deck at each snapshot (46 ms off the main thread), a
  space's counts when they do change (44 ms on it: every card's row is read), a deck chosen
  in a space (40 ms once).

## Built (2026-11-17): a card's look in its note

- A card was plain text in the live preview, but for its id. Its parts are styled now
  (`card_parts` finds them, `Style::CardDeck`, `CardQuestion`, `CardMark`): the deck's mark
  `#@deck/sub` in the accent colour on a plate, as inline code's; the question strong, so it
  stands apart from the answer; what parts them (`::`, the `?` of its line, both braces of a
  card in braces: the owner asked for the opening one grey too) muted. A cloze's `==words==` were highlighted already, as any.
- Nothing is hidden or replaced: the text is as typed, so a card is edited as any line. No
  frame around a card, no button in it.

## Built (2026-11-17): a card is a block, lilac

- Owner: «блоки питань по типу > блоків, тільки оберемо бузковий колір». A card that stands
  alone (not a list item's) shows as a quote does: a bar down its paragraphs, the blank ones
  inside braces too, its text indented past it; the bar and the deck's mark lilac (`look.card`,
  a step lighter in the dark look), where a quote's bar is the accent. `Highlight::cards` holds
  such cards' ranges; the bar is the quote's mark with a colour. Still nothing hidden.

## Built (2026-11-17): learning steps

- A new card graded Good went straight to its days (three). It is learned in steps first now,
  as the reference schedules them (py-fsrs 4.1.2, read for the rules) and as Anki does: a
  minute, then ten, then its days. Again goes to the first step, Hard stays at its step (at
  the first, between it and the next), Good to the next and past the last to its days, Easy to
  its days at once. A card forgotten after its days began is learned again in one step of ten
  minutes (what «Again → ten minutes» was). The memory is FSRS's as before: the steps move
  only when a card is due. `review` gives the schedule after one review (the fold and the
  grades' labels both use it); the reference's own test with its steps passes whole (0, 4, 14,
  44, 125, 328, 0, 0, 7, 16, 34, 71, 142 days).
- **A deck's setting**, the third: the steps' minutes («1 10»; «0»: none), its own or the
  nearest deck's above; `.knotes/decks.json` holds `"steps": [1, 10]`. The deck `""` is above
  every deck (the vault's own; no dialog for it). Without steps a new card's days begin at its
  first review, Again too (a day).
- **In a review** a card still being learned after its grade comes back at the review's end
  (before, only one graded Again did); when nothing else is left it is asked at once, not held
  for its minutes.
- Index schema 18 (`step`, `relearning`): every card is scheduled again from the logs with the
  steps. A card reviewed once and graded Good before this stands at its second step: it is due
  at once, and its next Good begins its days.
