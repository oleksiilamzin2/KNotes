# 0015. Search in the note list

Status: accepted, 2026-11-12

## Context

The owner asked for a search icon over the second column that searches the list. Choices
made with the owner: within the list the sidebar selected (the whole vault for "All notes");
first titles and text together, then Obsidian's operators with file names by default.

## Decision

- A magnifying glass in the toolbar over the list shows a search row at the top of the
  list column, under the toolbar; the list scrolls beneath it. A search field in the toolbar
  itself (NSSearchToolbarItem, 220 pt open) did not fit next to the list title and pushed the
  toolbar separator 100–170 pt past the list/note boundary.
- Operators as in Obsidian (owner's follow-ups): file names by default (`file:` or `files:`
  says so explicitly); `content:` finds notes holding every word anywhere in them, as
  Obsidian does; `block:` asks for every word in one block. File names: fuzzy title search,
  best first. Text: full-text search with word forms through the query stemmer, and every
  word also as the start of a word however short (`люц бап` finds "Люцифер" and
  "Баптист"), so halves of words work while typing. Each note once, best block first, the
  row showing the text around the match. Always within the selected list.
- Note-level search: each word's notes come from its own full-text match and are
  intersected; up to 1000 notes are ranked by bm25 over their matching blocks, larger sets
  show their first 500 notes unranked (ranking 7k notes took ~37 ms).
- A Files/Content switch in the search row rewrites the operator in the query, so the text
  stays the only truth; typing the operator moves the switch.
- The search runs in a background thread with its own read-only index connection; queries
  typed while it runs replace each other, and only the answer to the latest one is shown.
- Clicking the glass again hides the row and restores the list.

## Consequences

- Measured on 50k notes (M5 Pro, release): a search takes 10–40 ms in the background (the
  first title match after a pause is the slow part, ~25 ms); the main thread spends ~1.5 ms
  sending it and ~2–3 ms showing the results.
- Note-level text search at 50k (M5 Pro): p95 12.5 ms for two words, 14.4 ms for two
  three-letter word starts (about 25–29 ms on an M1, inside the 30 ms budget of ADR 0006
  with little room; the first try, filtering by temporary block tables and ranking every
  match, took 44–55 ms).
- No keyboard shortcut yet: Cmd+F is left for finding inside the open note.

## Search by meaning (2026-11-15)

At the owner's word the switch has a third segment, «За змістом», and the operator `meaning:`
(ADR 0028): the notes nearest the query in meaning, whatever their words, within the selected
list (the scope goes to the search by meaning, not filtered after it, so a folder's notes are
found however many nearer notes lie outside it), each once with its nearest block shown, at
most 40 blocks (two of a note). Notes kept from the AI are shown: the search is the user's own.
The model starts loading as soon as the mode is chosen. Without the model (the probe, a build
without it) nothing is found.

Ranked by meaning alone, "ліки від тиску" put the mother's note (her pressure and her pills)
fourth, behind the first-aid kit, a travel list and a pizza dough, and listed 34 notes: the
model scores every household note 0.2–0.35 against any query. The mode is therefore hybrid:
the notes holding the query's words (question and function words left out, word forms found)
come first, as search by words gives them, merged by reciprocal rank fusion with the notes near
in meaning, of which those below 0.25, or more than 0.10 behind the nearest, are left out. On
the household vault "ліки від тиску" then lists the mother's note first, "що робити якщо пропав
інтернет" the internet note first. The function (`knotes_tools::notes_by_meaning`) is measured
by the retrieval benchmark as a third run: by words 37 of 37, by meaning 54 of 65 (MRR 0.75),
5.5 documents listed a query; the thresholds were tuned there (a floor of 0.30 left five
queries, in other languages mostly, with nothing; 0.25 behind the nearest listed 16 documents a
query, 0.10 listed 5.5 and lost nothing, 0.07 lost an answer).

## The field in the place of the list's name (2026-11-17)

At the owner's word, once the list's name moved from the toolbar to the row under it: the
magnifying glass shows the search field in that row, in the place of the list's name and count
(both hidden while the search shows), not in a row under it. The Files/Content/By meaning
switch keeps a row of its own at the top of the list column, now 32 pt high instead of 64.

## The vault's search in a window of its own (2026-11-18)

- Owner: the toolbar's magnifying glass (beside the new note's button, after the tabs) «нехай
  відкриває окреме вікно пошуку: зверху перелік файлів пошуку по всій базі, а знизу на половину
  екрану превью», «як у DEVONthink». The list's search stays as it is, from the glass over the
  list; this one is the vault's, whatever list or space the window shows.
- **The window** (`SearchWindow`): a field on top; under it the documents found, a row each
  (the name, its folder at the right, the text around the find under it), those found by name
  first (as the title search ranks them), then those whose text holds every word (full-text,
  word starts), each document once; in the lower half the document chosen, shown as the vault's
  window shows a note (a note's own text view with the live preview, read only: headings,
  lists, quotes, code, cards; plain text at first, which the owner asked to be as the notes
  are), the query's words marked and the first scrolled into view (a PDF's as its indexed
  text). A link clicked there is followed from that document: the note or PDF it leads to
  opens in the vault's window (the web's in the browser) and the search closes; a link to no
  note makes none, as it would in a note open in the window. Every click there is as with
  Cmd: a link is followed in the cursor's paragraph too, and no task is ticked. Down
  and up choose, a click too; Return or a double click opens the document in the vault's
  window and closes the search; Escape closes it. Opened by the toolbar's glass or by Edit's
  «Пошук у базі…», ⌥⌘F (⇧⌘F stays the list's search). Opened again, it searches what was typed
  before in the vault as it is now.
- A document found has its marks after its name, as the list's rows and the tabs have them
  (`row_marks`): the dot of its space when that is not the window's, and the AI's mark when
  it is kept from the AI. The owner missed them in the first version.
- **What is searched, and the case** (the owner asked for both), under the field: «Усе» (the
  names, then the text; as it opens), «Файли» (the names), «Вміст» (the text); apart from
  them, after a gap (the owner's wish: it is another kind of search), «За змістом» (by
  meaning, ADR 0028), chosen in their place; and «Aa», on while the letters' case is ignored, as the list's
  search has it: off, only a name or a text holding each word as typed. The case has no part
  in a search by meaning, and «Aa» is not shown then (greyed while on, its letters could not
  be read).
- Names and text are searched on the main thread as the field's text changes, with a reader
  of its own: they take a few milliseconds at 10 000 notes (ADR 0006's measurements), and no
  answers arrive out of order. By meaning is asked of the list's search thread
  (`SearchWorker`, one of the window's own), which asks the embedding model; the list is
  empty until its answer comes, and an answer to an older question is dropped. No scope, no
  columns or sorting as DEVONthink's have.
- Later the same day: the document shown in the search's lower half shows no paragraph's markup
  for the cursor's sake (`LivePreview::show_only`: nothing is typed there, and the first
  heading stood with its `#`); and the list's «Aa» is drawn as the vault's search's, a switch
  of one segment, readable while on.
- **Every kind of the vault's search is the search thread's** (2026-11-18, an audit's findings
  11 and 12): the names and the text were searched on the main thread at each letter, which
  grows with the vault (every name looked at, a full-text query slowest at one or two
  letters, up to 200 notes read for their words); only by meaning went to `SearchWorker`.
  All go there now (`names_too` asks a search of the text for the names as well); an answer
  to an older question is dropped. Driven by the probe, the same search is run at once
  (`search_now`), the answer being wanted before the next step. The list of what is found
  starts at its top after each search, and is as wide as its window when that is resized.
- **The words are marked in the rows, and a document's finds gone through** (2026-11-18): a
  row draws its name and the text found with the query's words on a plate (yellow; paler on
  the chosen row's blue), so it shows why the document is there. The document shown has a
  count at the row of the kinds' right, «3 з 12», with two arrows: ⌘G and ⇧⌘G go to the next
  find and the one before, round the document's ends, each scrolled into view and pointed at
  with the system's find indicator. The words are found as they are marked in the preview:
  any case, anywhere in a word.
- **The query narrows itself** in that window (2026-11-18). A phrase in quotes must stand in
  the name or the text word for word (`"позовна давність"`); `-word` leaves out the documents
  whose name or text holds it; `tag:work` (`тег:`) keeps the notes with the tag or one under
  it; `folder:Авто` (`тека:`) those in a folder of that name anywhere in their path, several
  in a row as `Work/Проєкти`; `space:Дім` (`простір:`) those in the space of that name. A
  value with spaces goes in quotes (`folder:"Мої проєкти"`); these places narrow together, with
  `AND` between them or nothing (`tag:a AND tag:b`: both tags); `OR` between two makes them
  one or the other, of any kinds (`space:Дім OR space:Робота`, `tag:work OR folder:Авто`).
  The two are written in capitals. `AND` between words is left out (the words are all asked
  for anyway); `OR` between two plain words finds the documents with either (`radar OR budget план`: the
  plan, with either): the search's thread searches once for each way to take one word of
  every such list, sixteen at most, and lists what is found together, each document once;
  by meaning all the words are asked at once. `OR` beside a phrase or a word left out is
  searched as a word. With a
  tag, a folder or a space and no words, every document there is listed by its name. The
  phrases' words are searched by and marked as the others; what narrows is not marked.
  `Refine` (knotes-app) takes the query apart; the search's thread applies it after the names
  and the text are found (a tag before: only its notes are searched). The list's search row
  is not narrowed so: its scope is the list chosen in the sidebar.
- **What a search finds is found outside the window** (2026-11-18): `knotes_app::search` takes a
  request and the index and gives the names and the text found, narrowed; the app's search
  thread only calls it and hands the answer to the main thread. So its rules are tested
  without launching the app; the window's tests keep what only a window shows (the rows, the
  marks, the keys).
