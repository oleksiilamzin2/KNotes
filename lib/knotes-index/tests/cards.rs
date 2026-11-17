use knotes_cards::{Fsrs, Grade, review_line};
use knotes_core::{CardIndex, DeckCount, FileKind, IndexChange, IndexWriter, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

const DAY: i64 = 86_400;

fn put(index: &mut SqliteIndex, path: &str, text: &str) {
    let parsed = MarkdownParser::new().parse(text);
    let kind = if path.ends_with(".pdf") { FileKind::Pdf } else { FileKind::Markdown };
    let file = VaultFile { path: path.into(), kind, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &file, note: Some((text, &parsed)) }]).unwrap();
}

fn count(deck: &str, cards: usize, due: usize, fresh: usize) -> DeckCount {
    DeckCount { deck: deck.into(), cards, due, fresh }
}

#[test]
fn a_note_s_cards_are_counted_by_deck_and_follow_the_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови hello::привіт\n");
    put(&mut index, "b.md", "#@історія Коли?\n?\nТоді. ^c-3\n");
    put(&mut index, "doc.pdf", "#@pdf not::a card");
    // By code points (і comes after м): the window sorts them for showing.
    assert_eq!(index.deck_counts(0).unwrap(), [count("мови", 1, 0, 1), count("мови/англійська", 2, 0, 2), count("історія", 1, 0, 1)]);
    // Edited: a card gone, a deck with it.
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n");
    assert_eq!(index.deck_counts(0).unwrap(), [count("мови/англійська", 1, 0, 1), count("історія", 1, 0, 1)]);
    index.apply(&[IndexChange::Remove { path: "b.md" }]).unwrap();
    assert_eq!(index.deck_counts(0).unwrap(), [count("мови/англійська", 1, 0, 1)]);
}

#[test]
fn reviews_taken_from_the_logs_schedule_the_cards() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови hello::привіт ^c-3\n\n#@мови/німецька Hund::пес\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    // c-1 Good (due in 3 days), c-2 Again (due in ten minutes); the last line still being written.
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-2", Grade::Again) + "10000";
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs).unwrap(), 2);
    let read = index.reviews_read("mac.tsv").unwrap();
    assert_eq!(read as usize, log.len() - 5, "up to the last whole line");
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs).unwrap(), 0, "read again: nothing new");
    let decks = |until| index.deck_counts(until).unwrap();
    assert_eq!(decks(now + 3600), [count("мови", 1, 0, 1), count("мови/англійська", 2, 1, 0), count("мови/німецька", 1, 0, 1)]);
    assert_eq!(decks(now + 3 * DAY)[1], count("мови/англійська", 2, 2, 0));
    // What to review in a deck and those under it: the due first (the longest due first), then
    // the new, in the note's order.
    let due: Vec<(Option<String>, Option<i64>)> = index.due_cards(Some("мови"), now + 3 * DAY, 10).unwrap().into_iter().map(|c| (c.id, c.due)).collect();
    assert_eq!(due, [(Some("c-2".into()), Some(now + 600)), (Some("c-1".into()), Some(now + 3 * DAY)), (Some("c-3".into()), None), (None, None)]);
    let english: Vec<Option<String>> = index.due_cards(Some("мови/англійська"), now + 3600, 10).unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(english, [Some("c-2".to_owned())], "its own, and only the due");
    assert_eq!(index.due_cards(None, now + 3600, 2).unwrap().len(), 2, "every deck, the limit kept");
    let card = &index.due_cards(Some("мови/англійська"), now + 3600, 1).unwrap()[0];
    assert_eq!((card.path.as_str(), card.start, card.deck.as_str()), ("a.md", 50, "мови/англійська"), "where it is written");
}

#[test]
fn a_second_log_s_earlier_reviews_are_worked_in_in_time_s_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@a q::a ^c-1\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    index.take_reviews("mac.tsv", 0, &review_line(now, "c-1", Grade::Good), &fsrs).unwrap();
    let alone = index.due_cards(None, i64::MAX, 1).unwrap()[0].due.unwrap();
    // Another Mac reviewed it three days before; its log arrives later.
    index.take_reviews("other.tsv", 0, &review_line(now - 3 * DAY, "c-1", Grade::Good), &fsrs).unwrap();
    let both = index.due_cards(None, i64::MAX, 1).unwrap()[0].due.unwrap();
    assert!(both > alone, "a second review, not a first: due later ({alone} then {both})");
    // More of the first log, read from where it was.
    let read = index.reviews_read("mac.tsv").unwrap();
    index.take_reviews("mac.tsv", read, &review_line(both, "c-1", Grade::Easy), &fsrs).unwrap();
    assert!(index.due_cards(None, i64::MAX, 1).unwrap()[0].due.unwrap() > both);
}
