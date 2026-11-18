use knotes_app::{CardColumn, CardSearch, CardTable, Standing, card_of, replace_block, standing};
use knotes_cards::scan_cards;
use knotes_core::{CardRow, CardState, Reviewed};

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn card(path: &str, start: u32, deck: &str, id: Option<&str>, reviewed: Option<(i64, i64, u32, u32, Option<u8>)>) -> CardRow {
    let reviewed = reviewed.map(|(last, due, reviews, lapses, step)| Reviewed { state: CardState { stability: 1.0, difficulty: 5.0, last, step, relearning: false }, first: last, due, reviews, lapses });
    CardRow { path: path.into(), start, end: start + 10, deck: deck.into(), id: id.map(str::to_owned), reviewed }
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

