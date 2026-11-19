use knotes_app::{CardColumn, CardSearch, CardTable, MarkedRows, Standing, card_of, in_deck, block_to_delete, replace_block, standing};
use knotes_cards::scan_cards;
use knotes_core::{CardRow, CardState, Reviewed};

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn card(path: &str, start: u32, deck: &str, id: Option<&str>, reviewed: Option<(i64, i64, u32, u32, Option<u8>)>) -> CardRow {
    let reviewed = reviewed.map(|(last, due, reviews, lapses, step)| Reviewed { state: CardState { stability: 1.0, difficulty: 5.0, last, step, relearning: false }, first: last, due, reviews, lapses });
    CardRow { path: path.into(), start, end: start + 10, deck: deck.into(), id: id.map(str::to_owned), reviewed, paused: false }
}

/// Four cards as the index gives them, by note: one new, one learned, two with their days.
fn rows() -> Vec<CardRow> {
    vec![
        card("a.md", 0, "мови/10", Some("c-1"), Some((NOW - 4 * DAY, NOW + 6 * DAY, 3, 1, None))),
        card("a.md", 40, "мови/2", None, None),
        card("b.md", 0, "історія", Some("c-3"), Some((NOW - 60, NOW + 540, 1, 0, Some(0)))),
        card("b.md", 40, "мови/2", Some("c-4"), Some((NOW - DAY, NOW + DAY, 7, 0, None))),
    ]
}

fn ids(table: &CardTable) -> String {
    table.rows().iter().map(|row| row.id.clone().unwrap_or_else(|| "new".into())).collect::<Vec<_>>().join(" ")
}

#[test]
fn the_cards_are_sorted_by_a_column_those_never_reviewed_last() {
    let mut table = CardTable::new(rows());
    assert_eq!((ids(&table).as_str(), table.sorted()), ("c-1 new c-3 c-4", None), "as their notes have them");
    // By when they are due: the soonest first; again, the latest first. The new one is last
    // either way: it has no due day to stand by.
    table.sort_by(CardColumn::Due);
    assert_eq!((ids(&table).as_str(), table.sorted()), ("c-3 c-4 c-1 new", Some((CardColumn::Due, true))));
    table.sort_by(CardColumn::Due);
    assert_eq!((ids(&table).as_str(), table.sorted()), ("c-1 c-4 c-3 new", Some((CardColumn::Due, false))));
    // Another column starts upwards again.
    table.sort_by(CardColumn::Reviews);
    assert_eq!(ids(&table), "c-3 c-1 c-4 new");
    table.sort_by(CardColumn::Interval);
    assert_eq!(ids(&table), "c-3 c-4 c-1 new", "ten minutes, two days, ten days");
    table.sort_by(CardColumn::Lapses);
    assert_eq!(ids(&table), "c-3 c-4 c-1 new", "none forgotten keep their notes' order (b.md after a.md would be c-1 first, but c-1 was forgotten once)");
    // Decks as a person reads them (2 before 10); the same deck's by their notes.
    table.sort_by(CardColumn::Deck);
    assert_eq!(ids(&table), "c-3 new c-4 c-1");
    table.sort_by(CardColumn::Question);
    assert_eq!(ids(&table), "c-1 new c-3 c-4", "back to the notes' order");
    table.sort_by(CardColumn::Question);
    assert_eq!(ids(&table), "c-4 c-3 new c-1");
    // The vault changed: the new rows stand in the order the table is sorted by; the card
    // chosen is found again, by its id or, with none, by its place.
    table.sort_by(CardColumn::Reviews);
    let chosen = table.rows()[2].clone();
    let mut fresh = rows();
    fresh.remove(2);
    table.reload(fresh);
    assert_eq!((ids(&table).as_str(), table.position(&chosen)), ("c-1 c-4 new", Some(1)));
    assert_eq!(table.position(&rows()[1]), Some(2));
    assert_eq!(table.position(&rows()[2]), None, "gone");
    assert_eq!(table.position(&card("a.md", 44, "мови/2", None, None)), None, "a card with no id that moved is another card");
}

#[test]
fn a_card_stands_new_at_a_step_or_with_its_days() {
    let rows = rows();
    assert_eq!(standing(&rows[1]), Standing::New);
    assert_eq!(standing(&rows[2]), Standing::Learning(NOW + 540));
    assert_eq!(standing(&rows[0]), Standing::Review { due: NOW + 6 * DAY, days: 10.0 });
}

#[test]
fn a_card_s_text_is_replaced_where_it_stands_now_and_never_blind() {
    let note = "Intro\n\n#@en cat::кіт ^c-1\n\n#@en dog::пес\n\nEnd\n";
    let cards = scan_cards(note);
    let (cat, dog) = (&cards[0], &cards[1]);
    let block = |card: &knotes_cards::Card| &note[card.span.clone()];
    // The card of a row: by its id, or by its place.
    let row = card("a.md", cat.span.start as u32, "en", Some("c-1"), None);
    assert_eq!(card_of(&cards, &row).map(|c| c.front.as_str()), Some("cat"));
    assert_eq!(card_of(&cards, &CardRow { start: 0, ..row.clone() }).map(|c| c.front.as_str()), Some("cat"), "by its id wherever it is");
    let typed = card("a.md", dog.span.start as u32, "en", None, None);
    assert_eq!(card_of(&cards, &typed).map(|c| c.front.as_str()), Some("dog"));
    assert!(card_of(&cards, &CardRow { start: 3, ..typed }).is_none());
    // Replaced at its place; the rest of the note as it was.
    let changed = replace_block(note, cat.span.start, block(cat), "#@en cat::кішка ^c-1").unwrap();
    assert_eq!(changed, "Intro\n\n#@en cat::кішка ^c-1\n\n#@en dog::пес\n\nEnd\n");
    // The note grew above it since the table was filled: found where it stands now.
    let grown = format!("A new line.\n{note}");
    assert_eq!(replace_block(&grown, dog.span.start, block(dog), "#@en dog::собака").as_deref(), Some("A new line.\nIntro\n\n#@en cat::кіт ^c-1\n\n#@en dog::собака\n\nEnd\n"));
    // Changed meanwhile, or there twice: nothing is written.
    assert_eq!(replace_block("Intro\n\n#@en cat::кицька ^c-1\n", cat.span.start, block(cat), "x"), None);
    assert_eq!(replace_block(&format!("{}\n{}\n", block(dog), block(dog)), 99, block(dog), "x"), None, "which of the two?");
    assert_eq!(replace_block(&format!("{}\n{}\n", block(dog), block(dog)), 0, block(dog), "x").as_deref(), Some("x\n#@en dog::пес\n"), "at its own place it is that one");
    assert_eq!(replace_block(note, 0, "", "x"), None);
    assert_eq!(replace_block(note, 10_000, block(cat), "#@en a::b").as_deref(), Some("Intro\n\n#@en a::b\n\n#@en dog::пес\n\nEnd\n"), "a place past the note's end");
}

#[test]
fn a_card_changed_moves_those_after_it_in_its_note() {
    let mut table = CardTable::new(vec![card("a.md", 10, "en", None, None), card("a.md", 40, "en", None, None), card("a.md", 70, "en", None, None), card("b.md", 40, "en", None, None)]);
    // The second card of a.md grew by five bytes: it ends later, the third stands later, the
    // first and the other note's are where they were.
    table.block_changed("a.md", 40, 10, 15);
    let places: Vec<(u32, u32)> = table.rows().iter().map(|row| (row.start, row.end)).collect();
    assert_eq!(places, [(10, 20), (40, 55), (75, 85), (40, 50)]);
    // Made shorter again: back where they were.
    table.block_changed("a.md", 40, 15, 10);
    assert_eq!(table.rows().iter().map(|row| row.start).collect::<Vec<_>>(), [10, 40, 70, 40]);
}

#[test]
fn a_search_keeps_the_cards_of_a_deck_named_so_and_those_written_where_the_words_were_found() {
    let mut table = CardTable::new(rows());
    table.sort_by(CardColumn::Reviews);
    // The index found the words in a.md at 35..60 (the second card's block) and in b.md at
    // 100..120 (no card there).
    let found = || vec![("a.md".to_owned(), 35, 60), ("b.md".to_owned(), 100, 120)];
    table.search(&CardSearch::new("кіт", found()));
    assert_eq!((ids(&table).as_str(), table.count()), ("new", 4), "the card whose place the find lies over; all four are still the table's");
    // A deck's name, any case, every word: its cards, sorted as the table is.
    table.search(&CardSearch::new("МОВИ 2", Vec::new()));
    assert_eq!(ids(&table), "c-4 new");
    table.search(&CardSearch::new("мови", found()));
    assert_eq!(ids(&table), "c-1 c-4 new", "by its deck or by its text");
    table.search(&CardSearch::new("zzz", Vec::new()));
    assert_eq!((ids(&table).as_str(), table.count()), ("", 4));
    // A card changed while a part shows moves those after it in all of them.
    table.block_changed("a.md", 0, 10, 20);
    // Nothing typed: every card again, still sorted.
    table.search(&CardSearch::new("  ", found()));
    assert_eq!((ids(&table).as_str(), table.count()), ("c-3 c-1 c-4 new", 4));
    assert_eq!(table.rows().iter().find(|row| row.id.is_none()).map(|row| row.start), Some(50), "moved while it was not shown");
    // The vault changed under a search: every card shows until it is searched again.
    table.search(&CardSearch::new("історія", Vec::new()));
    table.reload(rows());
    assert_eq!((table.rows().len(), table.count()), (4, 4));
}

#[test]
fn a_card_is_paused_moved_to_another_deck_and_removed_in_its_row_at_once() {
    let mut table = CardTable::new(rows());
    // Paused: its row says so, whatever its schedule; a card with no id is given the one it
    // was paused by. The same in every card while a search shows a part.
    table.search(&CardSearch::new("мови 2", Vec::new()));
    let typed = table.rows().iter().find(|row| row.id.is_none()).unwrap().clone();
    table.set_paused(&typed, "c-9", true);
    table.search(&CardSearch::new("", Vec::new()));
    let paused = table.rows().iter().find(|row| row.id.as_deref() == Some("c-9")).unwrap();
    assert_eq!((paused.paused, standing(paused)), (true, Standing::Paused));
    let reviewed = table.rows()[0].clone();
    table.set_paused(&reviewed, "c-1", true);
    assert_eq!(standing(&table.rows()[0]), Standing::Paused, "a reviewed card too");
    table.set_paused(&reviewed, "c-1", false);
    assert!(matches!(standing(&table.rows()[0]), Standing::Review { .. }), "resumed, it stands as it stood");
    // Removed from its note: its row goes, the others stay.
    table.remove(&reviewed);
    assert_eq!((ids(&table).as_str(), table.count()), ("c-9 c-3 c-4", 3));
    // A card's text put in another deck: its mark's deck alone changes.
    assert_eq!(in_deck("#@мови/англійська cat::кіт ^c-1", "тварини").as_deref(), Some("#@тварини cat::кіт ^c-1"));
    assert_eq!(in_deck("#@en{\nQ\n?\nA\n} ^c-2", " en/verbs/ ").as_deref(), Some("#@en/verbs{\nQ\n?\nA\n} ^c-2"), "a card in braces; the name tidied");
    assert_eq!(in_deck("#@en Що?\n?\nТе.", "x").as_deref(), Some("#@x Що?\n?\nТе."));
    assert_eq!((in_deck("cat::кіт", "x"), in_deck("#@en a::b", " "), in_deck("#@en a::b", "two words"), in_deck("#@ a::b", "x")), (None, None, None, None));
}


#[test]
fn a_deleted_card_leaves_no_gap_where_it_was() {
    let without = |text: &str, block: &str| {
        let at = text.find(block).unwrap();
        let gone = block_to_delete(text, at..at + block.len());
        format!("{}{}", &text[..gone.start], &text[gone.end..])
    };
    assert_eq!(without("Intro\n\n#@en a::b\n\nEnd\n", "#@en a::b"), "Intro\n\nEnd\n", "of the two blank lines one stays");
    assert_eq!(without("Intro\n\n#@en a::b\n", "#@en a::b"), "Intro\n", "the last line: no blank line ends the note");
    assert_eq!(without("Intro\n\n#@en a::b", "#@en a::b"), "Intro\n");
    assert_eq!(without("#@en a::b\n\nEnd\n", "#@en a::b"), "End\n", "the first line");
    assert_eq!(without("#@en a::b\n", "#@en a::b"), "");
    assert_eq!(without("Intro\n#@en a::b\nEnd\n", "#@en a::b"), "Intro\nEnd\n", "a line among lines: it alone");
    assert_eq!(without("Intro\n\n#@en a::b\nEnd\n", "#@en a::b"), "Intro\n\nEnd\n", "the blank line before stays, with none after");
    assert_eq!(without("A word #@en {gap} more.\n", "#@en {gap}"), "A word more.\n", "inside a line: one space of the two");
    assert_eq!(block_to_delete("abc", 2..9), 2..9, "a place outside the text: as given");
}

#[test]
fn rows_are_chosen_together_by_clicks_with_a_key_held_and_by_arrows() {
    let mut marked = MarkedRows::default();
    assert_eq!(marked.with(Some(2)), [2], "one row alone");
    assert_eq!(marked.with(None), [] as [usize; 0]);
    // ⌘: one more, shown; clicked again, it goes and the first still chosen shows.
    assert_eq!(marked.toggle(Some(2), 5), 5);
    assert_eq!((marked.with(Some(5)), marked.holds(2), marked.holds(3)), (vec![2, 5], true, false));
    assert_eq!(marked.toggle(Some(5), 0), 0);
    assert_eq!(marked.toggle(Some(0), 0), 2, "the one shown, clicked off");
    assert_eq!(marked.with(Some(2)), [2, 5]);
    assert_eq!(marked.toggle(Some(2), 5), 2, "another, clicked off: the one shown stays");
    assert_eq!((marked.with(Some(2)), marked.holds(2)), (vec![2], false), "one alone again");
    assert_eq!(marked.toggle(Some(2), 2), 2, "the last one stays chosen");
    assert_eq!(marked.toggle(None, 4), 4);
    // ⇧: from the one shown to the one clicked, either way; on itself, it alone.
    marked.extend(4, 1);
    assert_eq!(marked.with(Some(4)), [1, 2, 3, 4]);
    marked.extend(4, 6);
    assert_eq!(marked.with(Some(4)), [4, 5, 6], "anew from the one shown");
    marked.extend(4, 4);
    assert_eq!(marked.with(Some(4)), [4]);
    // ⇧ with arrows: on from where the last went, back through the one shown, not past an end.
    assert_eq!((marked.step(4, true, 6), marked.step(4, true, 6), marked.step(4, true, 6)), (5, 5, 5));
    assert_eq!(marked.with(Some(4)), [4, 5]);
    assert_eq!((marked.step(4, false, 6), marked.step(4, false, 6)), (4, 3));
    assert_eq!(marked.with(Some(4)), [3, 4]);
    marked.clear();
    assert_eq!((marked.step(0, false, 6), marked.with(Some(0))), (0, vec![0]));
    // ⌘A.
    marked.all(3);
    assert_eq!(marked.with(Some(1)), [0, 1, 2]);
    marked.all(1);
    assert_eq!(marked.with(Some(0)), [0]);
}
