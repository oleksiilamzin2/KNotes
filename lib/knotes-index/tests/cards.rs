use knotes_cards::{DeckSettings, Fsrs, Grade, review_line, taken_back_line};
use knotes_core::{CardIndex, DeckCount, FileKind, IndexChange, IndexWriter, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

const DAY: i64 = 86_400;

/// No learning steps (a new card's days begin at its first review): these tests are of the
/// days; the steps have their own.
fn no_steps() -> DeckSettings {
    DeckSettings::parse(r#"{"": {"steps": []}}"#)
}

fn put(index: &mut SqliteIndex, path: &str, text: &str) {
    let parsed = MarkdownParser::new().parse(text);
    let kind = if path.ends_with(".pdf") { FileKind::Pdf } else { FileKind::Markdown };
    let file = VaultFile { path: path.into(), kind, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &file, note: Some((text, &parsed)) }]).unwrap();
}

fn count(deck: &str, cards: usize, due: usize, fresh: usize) -> DeckCount {
    DeckCount { deck: deck.into(), cards, due, fresh, learning: 0 }
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
    // c-1 Good (due in 3 days), c-2 Again (with no steps: due in a day); the last line still being written.
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-2", Grade::Again) + "10000";
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &no_steps()).unwrap(), 2);
    let read = index.reviews_read("mac.tsv").unwrap();
    assert_eq!(read as usize, log.len() - 5, "up to the last whole line");
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &no_steps()).unwrap(), 0, "read again: nothing new");
    let decks = |until| index.deck_counts(until).unwrap();
    assert_eq!(decks(now + DAY), [count("мови", 1, 0, 1), count("мови/англійська", 2, 1, 0), count("мови/німецька", 1, 0, 1)]);
    assert_eq!(decks(now + 3 * DAY)[1], count("мови/англійська", 2, 2, 0));
    // What to review in a deck and those under it: the due first (the longest due first), then
    // the new, in the note's order.
    let due: Vec<(Option<String>, Option<i64>)> = index.due_cards(Some("мови"), now + 3 * DAY, 10, 100, usize::MAX).unwrap().into_iter().map(|c| (c.id, c.due)).collect();
    assert_eq!(due, [(Some("c-2".into()), Some(now + DAY)), (Some("c-1".into()), Some(now + 3 * DAY)), (Some("c-3".into()), None), (None, None)]);
    let english: Vec<Option<String>> = index.due_cards(Some("мови/англійська"), now + DAY, 10, 100, usize::MAX).unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(english, [Some("c-2".to_owned())], "its own, and only the due");
    assert_eq!(index.due_cards(None, now + DAY, 2, 100, usize::MAX).unwrap().len(), 2, "every deck, the limit kept");
    let card = &index.due_cards(Some("мови/англійська"), now + DAY, 1, 100, usize::MAX).unwrap()[0];
    assert_eq!((card.path.as_str(), card.start, card.deck.as_str()), ("a.md", 50, "мови/англійська"), "where it is written");
}

#[test]
fn a_second_log_s_earlier_reviews_are_worked_in_in_time_s_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@a q::a ^c-1\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    index.take_reviews("mac.tsv", 0, &review_line(now, "c-1", Grade::Good), &fsrs, &no_steps()).unwrap();
    let alone = index.due_cards(None, i64::MAX, 1, 100, usize::MAX).unwrap()[0].due.unwrap();
    // Another Mac reviewed it three days before; its log arrives later.
    index.take_reviews("other.tsv", 0, &review_line(now - 3 * DAY, "c-1", Grade::Good), &fsrs, &no_steps()).unwrap();
    let both = index.due_cards(None, i64::MAX, 1, 100, usize::MAX).unwrap()[0].due.unwrap();
    assert!(both > alone, "a second review, not a first: due later ({alone} then {both})");
    // More of the first log, read from where it was.
    let read = index.reviews_read("mac.tsv").unwrap();
    index.take_reviews("mac.tsv", read, &review_line(both, "c-1", Grade::Easy), &fsrs, &no_steps()).unwrap();
    assert!(index.due_cards(None, i64::MAX, 1, 100, usize::MAX).unwrap()[0].due.unwrap() > both);
}

#[test]
fn card_rows_list_a_deck_s_or_a_note_s_cards_with_their_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови hello::привіт\n");
    put(&mut index, "b.md", "#@мовиці x::y ^c-2\n\n#@мови/німецька Hund::пес ^c-3\n");
    let now = 1_000_000;
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now + DAY, "c-1", Grade::Again);
    index.take_reviews("mac.tsv", 0, &log, &Fsrs::default(), &no_steps()).unwrap();
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
    let mut settings = DeckSettings::parse(r#"{"": {"steps": []}, "мови": {"retention": 0.8}}"#);
    assert!(!index.take_deck_settings(&settings.to_text(), &fsrs).unwrap(), "nothing reviewed yet: nothing to schedule again");
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-4", Grade::Good) + &review_line(now, "c-9", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &settings).unwrap();
    let due = |index: &SqliteIndex, id: &str| index.card_rows(None, None).unwrap().into_iter().find(|c| c.id.as_deref() == Some(id)).unwrap().reviewed.unwrap().due;
    // Good's first stability is 3.173 days: due at 90% in 3 days, at 80% (its deck above says) in 8.
    assert_eq!((due(&index, "c-1"), due(&index, "c-4")), (now + 8 * DAY, now + 3 * DAY));
    // The settings changed: every reviewed card is scheduled again, once.
    settings = DeckSettings::parse(r#"{"": {"steps": []}, "мови/англійська": {"retention": 0.95}, "історія": {"retention": 0.8}}"#);
    assert!(index.take_deck_settings(&settings.to_text(), &fsrs).unwrap());
    assert!(!index.take_deck_settings(&settings.to_text(), &fsrs).unwrap(), "the same again: nothing to do");
    assert_eq!((due(&index, "c-1"), due(&index, "c-4")), (now + DAY, now + 8 * DAY));
    // The day's new cards: those first reviewed since, by deck (one no note holds is no deck's).
    assert_eq!(index.introduced(now).unwrap(), [("мови/англійська".to_owned(), 1), ("історія".to_owned(), 1)]);
    assert_eq!(index.introduced(now + 1).unwrap(), []);
    // Of the never reviewed, as many as asked; the due all the same.
    let ids = |new| -> Vec<Option<String>> { index.due_cards(Some("мови"), now + 30 * DAY, 10, new, usize::MAX).unwrap().into_iter().map(|c| c.id).collect() };
    assert_eq!(ids(0), [Some("c-1".to_owned())]);
    assert_eq!(ids(2), [Some("c-1".to_owned()), Some("c-2".into()), Some("c-3".into())]);
    assert_eq!(ids(9).len(), 4);
    assert_eq!(index.due_cards(Some("мови"), now + 30 * DAY, 2, 9, usize::MAX).unwrap().len(), 2, "the limit of all kept");
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
    index.take_reviews("mac.tsv", 0, &log, &Fsrs::default(), &no_steps()).unwrap();
    let stats = index.deck_stats("мови", day, &|_| true).unwrap();
    assert_eq!((stats.cards, stats.fresh, stats.young, stats.mature), (4, 1, 3, 0), "forgotten, c-1 is young again");
    assert_eq!((stats.due_by_day.len(), stats.due_by_day[0], stats.due_by_day[2], stats.due_by_day.iter().sum::<usize>()), (31, 2, 1, 3), "today's with the overdue, then by day");
    assert_eq!(stats.reviews_by_day.len(), 30);
    assert_eq!((stats.reviews_by_day[29], stats.reviews_by_day[28], stats.reviews_by_day[24]), ((1, 1), (1, 0), (1, 0)), "today last; the other deck's not counted");
    assert_eq!(stats.reviews_by_day.iter().map(|d| d.0).sum::<usize>(), 3, "older than thirty days: not there");
    assert_eq!(stats.grades, [1, 0, 2, 0], "Again once, Good twice");
    let english = index.deck_stats("мови/англійська", day, &|_| true).unwrap();
    assert_eq!((english.cards, english.fresh, english.young), (2, 0, 2));
    assert_eq!(index.deck_stats("нема", day, &|_| true).unwrap().cards, 0);
    // A part of the vault's notes (a space's): the cards and the reviews of the others left out.
    put(&mut index, "b.md", "#@мови x::y ^c-7\n");
    index.take_reviews("other.tsv", 0, &review_line(now, "c-7", Grade::Easy), &Fsrs::default(), &no_steps()).unwrap();
    let all = index.deck_stats("мови", day, &|_| true).unwrap();
    let part = index.deck_stats("мови", day, &|path| path == "a.md").unwrap();
    assert_eq!((all.cards, all.reviews_by_day[29], all.grades), (5, (2, 1), [1, 0, 2, 1]));
    assert_eq!(part, stats, "as before the other note came");
}

#[test]
fn a_new_card_is_learned_in_its_deck_s_steps() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови cat::кіт ^c-1\n\n#@мови dog::пес ^c-2\n\n#@історія Коли?::Тоді ^c-3\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    // The defaults (a minute, ten minutes) but for історія, which has none.
    let settings = DeckSettings::parse(r#"{"історія": {"steps": []}}"#);
    let log = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-2", Grade::Again) + &review_line(now, "c-3", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &settings).unwrap();
    let stands = |index: &SqliteIndex, id: &str| {
        let state = index.card_state(id).unwrap().unwrap();
        let due = index.card_rows(None, None).unwrap().into_iter().find(|c| c.id.as_deref() == Some(id)).unwrap().reviewed.unwrap().due;
        (due - now, state.step, state.relearning)
    };
    assert_eq!(stands(&index, "c-1"), (600, Some(1), false), "Good: its second step");
    assert_eq!(stands(&index, "c-2"), (60, Some(0), false), "Again: its first");
    assert_eq!(stands(&index, "c-3"), (3 * DAY, None, false), "no steps: its days");
    // Good again ten minutes later: its days begin.
    index.take_reviews("mac.tsv", log.len() as u64, &review_line(now + 600, "c-1", Grade::Good), &fsrs, &settings).unwrap();
    let (wait, step, _) = stands(&index, "c-1");
    assert_eq!((wait, step), (600 + 4 * DAY, None));
    // The deck's steps taken away: scheduled again, its first Good began its days.
    assert!(index.take_deck_settings(r#"{"мови": {"steps": []}}"#, &fsrs).unwrap());
    assert_eq!(stands(&index, "c-2"), (DAY, None, false), "Again with no step: its days too, the shortest");
}

#[test]
fn a_review_taken_back_is_gone_and_its_card_stands_as_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови cat::кіт ^c-1\n\n#@мови dog::пес ^c-2\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    // c-1 graded once and that taken back: new again. c-2 graded twice, the second taken back.
    let log = review_line(now, "c-1", Grade::Easy)
        + &taken_back_line(now, "c-1")
        + &review_line(now, "c-2", Grade::Good)
        + &review_line(now + 600, "c-2", Grade::Again)
        + &taken_back_line(now + 600, "c-2");
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &no_steps()).unwrap(), 5, "each line changed something");
    assert_eq!(index.card_state("c-1").unwrap(), None, "never reviewed, as far as the schedule goes");
    assert_eq!(index.deck_counts(now).unwrap()[0], count("мови", 2, 0, 1));
    let dog = index.card_rows(None, None).unwrap().into_iter().find(|c| c.id.as_deref() == Some("c-2")).unwrap().reviewed.unwrap();
    assert_eq!((dog.reviews, dog.lapses, dog.due), (1, 0, now + 3 * DAY), "as after its Good alone");
    // The whole log read again (the index built anew): the same.
    assert_eq!(index.take_reviews("mac.tsv", 0, &log, &fsrs, &no_steps()).unwrap(), 4, "the two taken back are put in and taken back again");
    assert_eq!(index.card_state("c-1").unwrap(), None);
    // The card graded anew after the undo: that review stands.
    let more = review_line(now + 5, "c-1", Grade::Good);
    index.take_reviews("mac.tsv", log.len() as u64, &more, &fsrs, &no_steps()).unwrap();
    assert_eq!(index.card_state("c-1").unwrap().map(|s| s.last), Some(now + 5));
}

#[test]
fn a_card_paused_is_asked_no_more_until_it_is_resumed() {
    use knotes_cards::paused_line;
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови cat::кіт ^c-1\n\n#@мови dog::пес ^c-2\n\n#@мови fox::лис ^c-3\n");
    let fsrs = Fsrs::default();
    let now = 1_000_000;
    // cat reviewed and due again, dog reviewed and not due yet, fox new.
    let log = review_line(now - 10 * DAY, "c-1", Grade::Good) + &review_line(now, "c-2", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &no_steps()).unwrap();
    assert_eq!(index.deck_counts(now).unwrap()[0], count("мови", 3, 1, 1));
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_default()).collect::<Vec<_>>().join(" ");
    assert_eq!((ids(index.due_cards(Some("мови"), now, 10, 10, usize::MAX).unwrap()), ids(index.cards_ahead("мови", now, 10).unwrap())), ("c-1 c-3".to_owned(), "c-2".to_owned()));
    // All three paused: they are the deck's still, but none waits, is asked or comes ahead.
    let paused: String = ["c-1", "c-2", "c-3"].iter().map(|id| paused_line(now + 1, id, true)).collect();
    assert_eq!(index.take_reviews("mac.tsv", log.len() as u64, &paused, &fsrs, &no_steps()).unwrap(), 3);
    assert_eq!(index.deck_counts(now).unwrap()[0], count("мови", 3, 0, 0));
    assert_eq!((ids(index.due_cards(Some("мови"), now, 10, 10, usize::MAX).unwrap()), ids(index.due_cards(None, now, 10, 10, usize::MAX).unwrap()), ids(index.cards_ahead("мови", now, 10).unwrap())), (String::new(), String::new(), String::new()));
    let rows = index.card_rows(None, None).unwrap();
    assert!(rows.iter().all(|row| row.paused) && rows[0].reviewed.is_some(), "its schedule is kept while it is paused");
    // Resumed, it stands as it stood.
    let resumed = paused_line(now + 2, "c-1", false) + &paused_line(now + 2, "c-3", false);
    index.take_reviews("mac.tsv", (log.len() + paused.len()) as u64, &resumed, &fsrs, &no_steps()).unwrap();
    assert_eq!(index.deck_counts(now).unwrap()[0], count("мови", 3, 1, 1));
    assert_eq!(ids(index.due_cards(Some("мови"), now, 10, 10, usize::MAX).unwrap()), "c-1 c-3");
    assert_eq!(index.card_rows(None, None).unwrap().iter().map(|row| row.paused).collect::<Vec<_>>(), [false, true, false]);
    // Another Mac's log arrives late with an older pause of cat: the newer line stands. Its
    // newer pause of fox takes.
    let late = paused_line(now, "c-1", true) + &paused_line(now + 9, "c-3", true);
    assert_eq!(index.take_reviews("other.tsv", 0, &late, &fsrs, &no_steps()).unwrap(), 1, "the older line changes nothing");
    assert_eq!(index.card_rows(None, None).unwrap().iter().map(|row| row.paused).collect::<Vec<_>>(), [false, true, true]);
    // The logs read again from their start, in another order (the index built anew): the same.
    index.take_reviews("other.tsv", 0, &late, &fsrs, &no_steps()).unwrap();
    index.take_reviews("mac.tsv", 0, &(log + &paused + &resumed), &fsrs, &no_steps()).unwrap();
    assert_eq!(index.card_rows(None, None).unwrap().iter().map(|row| row.paused).collect::<Vec<_>>(), [false, true, true]);
    // Paused and resumed in one second, in two logs: resumed, whichever log is read first.
    for order in [["one.tsv", "two.tsv"], ["two.tsv", "one.tsv"]] {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        put(&mut index, "a.md", "#@мови cat::кіт ^c-1\n");
        for name in order {
            index.take_reviews(name, 0, &paused_line(now, "c-1", name == "one.tsv"), &fsrs, &no_steps()).unwrap();
        }
        assert!(!index.card_rows(None, None).unwrap()[0].paused, "{order:?}");
    }
    // A card with no review at all can be paused: it is not new to ask.
    assert_eq!(index.card_state("c-3").unwrap(), None);
}

#[test]
fn of_the_cards_due_a_day_s_reviews_are_given_and_those_at_a_step_besides() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови a::1 ^c-1\n\n#@мови b::2 ^c-2\n\n#@мови c::3 ^c-3\n\n#@мови d::4 ^c-4\n\n#@мови e::5 ^c-5\n");
    let (fsrs, settings) = (Fsrs::default(), DeckSettings::default());
    let now = 100 * DAY;
    // Three cards whose days have begun, due for two weeks (Easy a month ago); one still at
    // its step, due in ten minutes (Good a minute ago); one new.
    let log = review_line(now - 30 * DAY, "c-1", Grade::Easy) + &review_line(now - 30 * DAY + 1, "c-2", Grade::Easy) + &review_line(now - 30 * DAY + 2, "c-3", Grade::Easy) + &review_line(now - 60, "c-4", Grade::Good);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &settings).unwrap();
    let soon = now + 600;
    assert_eq!(index.deck_counts(soon).unwrap()[0], DeckCount { deck: "мови".into(), cards: 5, due: 4, fresh: 1, learning: 1 });
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_default()).collect::<Vec<_>>().join(" ");
    assert_eq!(ids(index.due_cards(Some("мови"), soon, 10, 10, usize::MAX).unwrap()), "c-1 c-2 c-3 c-4 c-5", "no limit: all, the longest due first, the new one last");
    assert_eq!(ids(index.due_cards(Some("мови"), soon, 10, 10, 2).unwrap()), "c-1 c-2 c-4 c-5", "two of the three whose days have begun; the one at its step besides");
    assert_eq!(ids(index.due_cards(Some("мови"), soon, 10, 0, 0).unwrap()), "c-4", "the day's reviews taken: a card begun is still finished");
    assert_eq!(ids(index.due_cards(Some("мови"), soon, 2, 10, 5).unwrap()), "c-1 c-2", "no more than asked for at once");
    // What today has taken: the cards reviewed since it began whose days had begun before
    // (not the one first learned today), once they are at no step.
    let today = now - 3_600;
    assert!(index.reviewed(today).unwrap().is_empty(), "the card at its step was first reviewed today");
    let more = review_line(now, "c-1", Grade::Good) + &review_line(now, "c-2", Grade::Again);
    index.take_reviews("mac.tsv", log.len() as u64, &more, &fsrs, &settings).unwrap();
    assert_eq!(index.reviewed(today).unwrap(), [("мови".to_owned(), 1)], "c-1 is done; c-2, forgotten, is at a step again and comes back whatever the day takes");
    assert_eq!(ids(index.due_cards(Some("мови"), soon, 10, 0, 0).unwrap()), "c-4 c-2", "both at a step, the sooner due first");
}

#[test]
fn every_deck_together_is_read_as_its_decks_are_one_by_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "a.md", "#@мови a::1 ^c-1\n\n#@мови/англійська b::2 ^c-2\n\n#@історія c::3 ^c-3\n");
    put(&mut index, "b.md", "#@історія d::4 ^c-4\n\n#@історія e::5 ^c-5\n\n#@мови f::6\n");
    let (fsrs, settings) = (Fsrs::default(), DeckSettings::default());
    let now = 100 * DAY;
    // c-3 due the longest, then c-1, then c-2; c-4 at a step, due soon; c-5 reviewed and not
    // due for long; the last card new.
    let log = review_line(now - 40 * DAY, "c-3", Grade::Easy) + &review_line(now - 35 * DAY, "c-1", Grade::Easy) + &review_line(now - 30 * DAY, "c-2", Grade::Easy) + &review_line(now - 60, "c-4", Grade::Good) + &review_line(now - DAY, "c-5", Grade::Easy);
    index.take_reviews("mac.tsv", 0, &log, &fsrs, &settings).unwrap();
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_else(|| "new".into())).collect::<Vec<_>>().join(" ");
    let soon = now + 600;
    // Every deck's due cards, the longest due first, whichever deck they are of; none asked for
    // and the deck `""` are the same.
    assert_eq!(ids(index.due_cards(None, soon, 10, 0, usize::MAX).unwrap()), "c-3 c-1 c-2 c-4");
    assert_eq!(ids(index.due_cards(Some(""), soon, 10, 0, usize::MAX).unwrap()), "c-3 c-1 c-2 c-4");
    assert_eq!(ids(index.due_cards(None, soon, 2, 0, usize::MAX).unwrap()), "c-3 c-1", "as far as the limit");
    assert_eq!(ids(index.due_cards(None, soon, 10, 0, 1).unwrap()), "c-3 c-4", "a day's reviews, and the one at its step besides");
    assert_eq!(ids(index.due_cards(None, soon, 10, 5, usize::MAX).unwrap()), "c-3 c-1 c-2 c-4 new");
    assert_eq!(ids(index.due_cards(Some("історія"), i64::MIN, 10, 5, 0).unwrap()), "", "a deck's new cards alone, when it has none");
    assert_eq!(ids(index.due_cards(Some("мови"), i64::MIN, 10, 5, 0).unwrap()), "new");
    // Those not due yet, the soonest first.
    assert_eq!(ids(index.cards_ahead("", soon, 10).unwrap()), "c-5");
    assert_eq!(ids(index.cards_ahead("", now - 50 * DAY, 2).unwrap()), "c-3 c-1");
    // Paused, a card is in neither.
    index.take_reviews("mac.tsv", log.len() as u64, &knotes_cards::paused_line(now, "c-3", true), &fsrs, &settings).unwrap();
    assert_eq!(ids(index.due_cards(None, soon, 10, 0, usize::MAX).unwrap()), "c-1 c-2 c-4");
    assert_eq!(ids(index.cards_ahead("", now - 50 * DAY, 2).unwrap()), "c-1 c-2");
    // The statistics of every deck together are those of its decks added up.
    let (all, a, b) = (index.deck_stats("", now, &|_| true).unwrap(), index.deck_stats("мови", now, &|_| true).unwrap(), index.deck_stats("історія", now, &|_| true).unwrap());
    assert_eq!((all.cards, all.fresh, all.young, all.mature), (a.cards + b.cards, a.fresh + b.fresh, a.young + b.young, a.mature + b.mature));
    assert_eq!(all.cards, 6);
    let added = |one: &[usize], other: &[usize]| one.iter().zip(other).map(|(x, y)| x + y).collect::<Vec<_>>();
    assert_eq!(all.due_by_day, added(&a.due_by_day, &b.due_by_day));
    assert_eq!(all.grades.to_vec(), added(&a.grades, &b.grades));
    assert_eq!(all.reviews_by_day.iter().map(|d| d.0).sum::<usize>(), 2, "the reviews of the last thirty days: the one at a step and yesterday's");
    // A part of the notes (a space's): the statistics of those.
    assert_eq!(index.deck_stats("", now, &|path| path == "b.md").unwrap().cards, 3);
}

