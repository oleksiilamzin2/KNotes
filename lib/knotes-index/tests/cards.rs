use knotes_cards::{DeckSettings, Fsrs, Grade, review_line};
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
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &DeckSettings::default()).unwrap(), 2);
    let read = index.reviews_read("mac.tsv").unwrap();
    assert_eq!(read as usize, log.len() - 5, "up to the last whole line");
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &DeckSettings::default()).unwrap(), 0, "read again: nothing new");
    let decks = |until| index.deck_counts(until).unwrap();
    assert_eq!(decks(now + 3600), [count("мови", 1, 0, 1), count("мови/англійська", 2, 1, 0), count("мови/німецька", 1, 0, 1)]);
    assert_eq!(decks(now + 3 * DAY)[1], count("мови/англійська", 2, 2, 0));
    // What to review in a deck and those under it: the due first (the longest due first), then
    // the new, in the note's order.
    let due: Vec<(Option<String>, Option<i64>)> = index.due_cards(Some("мови"), now + 3 * DAY, 10, 100).unwrap().into_iter().map(|c| (c.id, c.due)).collect();
    assert_eq!(due, [(Some("c-2".into()), Some(now + 600)), (Some("c-1".into()), Some(now + 3 * DAY)), (Some("c-3".into()), None), (None, None)]);
    let english: Vec<Option<String>> = index.due_cards(Some("мови/англійська"), now + 3600, 10, 100).unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(english, [Some("c-2".to_owned())], "its own, and only the due");
    assert_eq!(index.due_cards(None, now + 3600, 2, 100).unwrap().len(), 2, "every deck, the limit kept");
    let card = &index.due_cards(Some("мови/англійська"), now + 3600, 1, 100).unwrap()[0];
    assert_eq!((card.path.as_str(), card.start, card.deck.as_str()), ("a.md", 50, "мови/англійська"), "where it is written");
}

#[test]
fn a_second_log_s_earlier_reviews_are_worked_in_in_time_s_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@a q::a ^c-1\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    index.take_reviews("mac.tsv", 0, &review_line(now, "c-1", Grade::Good), &fsrs, &DeckSettings::default()).unwrap();
    let alone = index.due_cards(None, i64::MAX, 1, 100).unwrap()[0].due.unwrap();
    // Another Mac reviewed it three days before; its log arrives later.
    index.take_reviews("other.tsv", 0, &review_line(now - 3 * DAY, "c-1", Grade::Good), &fsrs, &DeckSettings::default()).unwrap();
    let both = index.due_cards(None, i64::MAX, 1, 100).unwrap()[0].due.unwrap();
    assert!(both > alone, "a second review, not a first: due later ({alone} then {both})");
    // More of the first log, read from where it was.
    let read = index.reviews_read("mac.tsv").unwrap();
    index.take_reviews("mac.tsv", read, &review_line(both, "c-1", Grade::Easy), &fsrs, &DeckSettings::default()).unwrap();
    assert!(index.due_cards(None, i64::MAX, 1, 100).unwrap()[0].due.unwrap() > both);
}

#[test]
fn card_rows_list_a_deck_s_or_a_note_s_cards_with_their_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови hello::привіт\n");
    put(&mut index, "b.md", "#@мовиці x::y ^c-2\n\n#@мови/німецька Hund::пес ^c-3\n");
    let now = 1_000_000;
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now + DAY, "c-1", Grade::Again);
    index.take_reviews("mac.tsv", 0, &log, &Fsrs::default(), &DeckSettings::default()).unwrap();
    let ids = |deck, path| -> Vec<Option<String>> { index.card_rows(deck, path).unwrap().into_iter().map(|c| c.id).collect() };
    assert_eq!(ids(None, None), [Some("c-1".to_owned()), None, Some("c-2".into()), Some("c-3".into())], "by note, in its order");
    assert_eq!(ids(Some("мови"), None), [Some("c-1".to_owned()), None, Some("c-3".into())], "the deck and those under it, not one its name starts");
    assert_eq!(ids(Some("мови"), Some("b.md")), [Some("c-3".to_owned())]);
    let rows = index.card_rows(None, Some("a.md")).unwrap();
    let cat = rows[0].reviewed.unwrap();
    assert_eq!((cat.reviews, cat.lapses, cat.state.last, cat.due), (2, 1, now + DAY, now + DAY + 600));
    assert_eq!((rows[1].reviewed, rows[1].deck.as_str(), rows[1].start), (None, "мови", 50));
}

#[test]
fn a_deck_s_settings_move_when_its_cards_are_due_and_limit_its_new_cards() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови hello::привіт ^c-3\n\n#@історія Коли?::Тоді ^c-4\n\n#@мови bye::бувай\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    let mut settings = DeckSettings::parse(r#"{"мови": {"retention": 0.8}}"#);
    assert!(!index.take_deck_settings(&settings.to_text(), &fsrs).unwrap(), "nothing reviewed yet: nothing to schedule again");
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-4", Grade::Good) + &review_line(now, "c-9", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &settings).unwrap();
    let due = |index: &SqliteIndex, id: &str| index.card_rows(None, None).unwrap().into_iter().find(|c| c.id.as_deref() == Some(id)).unwrap().reviewed.unwrap().due;
    // Good's first stability is 3.173 days: due at 90% in 3 days, at 80% (its deck above says) in 8.
    assert_eq!((due(&index, "c-1"), due(&index, "c-4")), (now + 8 * DAY, now + 3 * DAY));
    // The settings changed: every reviewed card is scheduled again, once.
    settings = DeckSettings::parse(r#"{"мови/англійська": {"retention": 0.95}, "історія": {"retention": 0.8}}"#);
    assert!(index.take_deck_settings(&settings.to_text(), &fsrs).unwrap());
    assert!(!index.take_deck_settings(&settings.to_text(), &fsrs).unwrap(), "the same again: nothing to do");
    assert_eq!((due(&index, "c-1"), due(&index, "c-4")), (now + DAY, now + 8 * DAY));
    // The day's new cards: those first reviewed since, by deck (one no note holds is no deck's).
    assert_eq!(index.introduced(now).unwrap(), [("мови/англійська".to_owned(), 1), ("історія".to_owned(), 1)]);
    assert_eq!(index.introduced(now + 1).unwrap(), []);
    // Of the never reviewed, as many as asked; the due all the same.
    let ids = |new| -> Vec<Option<String>> { index.due_cards(Some("мови"), now + 30 * DAY, 10, new).unwrap().into_iter().map(|c| c.id).collect() };
    assert_eq!(ids(0), [Some("c-1".to_owned())]);
    assert_eq!(ids(2), [Some("c-1".to_owned()), Some("c-2".into()), Some("c-3".into())]);
    assert_eq!(ids(9).len(), 4);
    assert_eq!(index.due_cards(Some("мови"), now + 30 * DAY, 2, 9).unwrap().len(), 2, "the limit of all kept");
    // Ahead of the schedule: the reviewed ones not due yet, the deck's and those under it.
    let ahead = |deck, after| -> Vec<Option<String>> { index.cards_ahead(deck, after, 10).unwrap().into_iter().map(|c| c.id).collect() };
    assert_eq!((ahead("мови", now), ahead("мови", now + 2 * DAY), ahead("історія", now)), (vec![Some("c-1".to_owned())], vec![], vec![Some("c-4".to_owned())]));
}

#[test]
fn a_deck_s_statistics_count_its_cards_its_reviews_by_day_and_what_comes_due() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови hello::привіт ^c-3\n\n#@історія Коли?::Тоді ^c-4\n\n#@мови bye::бувай\n");
    // Today began at `day`; it is noon.
    let day = 100 * DAY;
    let now = day + DAY / 2;
    // c-1: Good 40 days ago, Easy 37 days ago (mature, due far off); Again today (due in ten minutes).
    // c-2: Good yesterday (young, due in two days). c-3: Good 5 days ago (due the day before yesterday: overdue).
    let log = review_line(now - 40 * DAY, "c-1", Grade::Good)
        + &review_line(now - 37 * DAY, "c-1", Grade::Easy)
        + &review_line(now, "c-1", Grade::Again)
        + &review_line(now - DAY, "c-2", Grade::Good)
        + &review_line(now - 5 * DAY, "c-3", Grade::Good)
        + &review_line(now, "c-4", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &Fsrs::default(), &DeckSettings::default()).unwrap();
    let stats = index.deck_stats("мови", day).unwrap();
    assert_eq!((stats.cards, stats.fresh, stats.young, stats.mature), (4, 1, 3, 0), "forgotten, c-1 is young again");
    assert_eq!((stats.due_by_day.len(), stats.due_by_day[0], stats.due_by_day[2], stats.due_by_day.iter().sum::<usize>()), (31, 2, 1, 3), "today's with the overdue, then by day");
    assert_eq!(stats.reviews_by_day.len(), 30);
    assert_eq!((stats.reviews_by_day[29], stats.reviews_by_day[28], stats.reviews_by_day[24]), ((1, 1), (1, 0), (1, 0)), "today last; the other deck's not counted");
    assert_eq!(stats.reviews_by_day.iter().map(|d| d.0).sum::<usize>(), 3, "older than thirty days: not there");
    assert_eq!(stats.grades, [1, 0, 2, 0], "Again once, Good twice");
    let english = index.deck_stats("мови/англійська", day).unwrap();
    assert_eq!((english.cards, english.fresh, english.young), (2, 0, 2));
    assert_eq!(index.deck_stats("нема", day).unwrap().cards, 0);
}
