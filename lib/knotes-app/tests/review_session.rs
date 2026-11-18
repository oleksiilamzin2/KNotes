use knotes_app::{ReviewSession, card_written};
use knotes_cards::{DeckSettings, Grade, scan_cards};
use knotes_core::{CardState, DueCard};

const NOW: i64 = 1_800_000_000;

/// The cards of a note as the index gives them to a review.
fn due(path: &str, text: &str) -> Vec<DueCard> {
    scan_cards(text).into_iter().map(|c| DueCard { path: path.into(), start: c.span.start as u32, end: c.span.end as u32, deck: c.deck, id: c.id, due: None }).collect()
}

/// No card is in the index: all are new.
fn new(_: &str) -> Option<CardState> {
    None
}

const NOTE: &str = "#@en cat::кіт ^c-aaa111\n#@en dog::пес ^c-bbb222\n#@en cow::корова ^c-ccc333\n";

#[test]
fn a_review_asks_its_cards_in_turn_each_id_once_and_passes_over_those_gone() {
    let mut cards = due("a.md", NOTE);
    assert_eq!(cards.len(), 3, "{cards:?}");
    // The same id written twice (a note duplicated): asked once. A card ahead of the schedule
    // comes after those due.
    let copy = DueCard { path: "copy.md".into(), ..cards[0].clone() };
    let ahead = cards.pop().unwrap();
    let mut review = ReviewSession::start("en", [cards, vec![copy]].concat(), vec![ahead]);
    assert_eq!((review.deck(), review.left(), review.reviewed(), review.current().is_none(), review.can_undo()), ("en", 3, 0, true, false));
    // The second card was deleted from its note since: passed over.
    let now_written = NOTE.replace("#@en dog::пес ^c-bbb222\n", "");
    let read = |path: &str| (path == "a.md").then(|| now_written.clone());
    review.next(read);
    let asked = review.current().unwrap().clone();
    assert_eq!((asked.front.as_str(), asked.back.as_str(), asked.answered, review.left()), ("cat", "кіт", false, 3));
    review.next(read);
    let asked = review.current().unwrap();
    assert_eq!((asked.front.as_str(), review.left()), ("cow", 1), "the one gone is not asked; found where it now stands");
    assert_eq!(asked.card.start as usize, now_written.find("#@en cow").unwrap());
    review.next(read);
    assert!(review.current().is_none() && review.left() == 0, "done");
}

#[test]
fn a_card_still_learned_comes_back_at_the_review_s_end_and_is_graded_from_where_the_review_left_it() {
    let settings = DeckSettings::default();
    let read = |_: &str| Some(NOTE.to_owned());
    let mut review = ReviewSession::start("en", due("a.md", NOTE), Vec::new());
    review.next(read);
    // Nothing to grade until the answer shows; then what each grade would do.
    assert!(review.grade(&settings, NOW, Grade::Good, new).is_none() && review.would_come_back(&settings, NOW, new).is_none());
    review.show_answer();
    let would = review.would_come_back(&settings, NOW, new).unwrap();
    assert!(would[0].due <= would[2].due && would[2].due < would[3].due, "Again soonest, Easy latest: {would:?}");
    // Good on a new card: at its first step, so it comes back in this review.
    let first = review.grade(&settings, NOW, Grade::Good, new).unwrap();
    assert_eq!((first.at, first.id.as_str(), first.before, first.after), (NOW, "c-aaa111", None, would[2]));
    assert!(first.after.step.is_some());
    review.graded(first.clone());
    assert_eq!((review.reviewed(), review.left(), review.can_undo()), (1, 4, true), "cat waits again behind dog and cow, and is still shown");
    review.next(read);
    assert_eq!((review.current().unwrap().front.as_str(), review.left()), ("dog", 3));
    // Easy takes a card out of its steps: it does not come back.
    review.show_answer();
    let easy = review.grade(&settings, NOW, Grade::Easy, new).unwrap();
    assert!(easy.after.step.is_none());
    review.graded(easy);
    review.next(read);
    assert_eq!((review.current().unwrap().front.as_str(), review.left()), ("cow", 2));
    review.show_answer();
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    review.next(read);
    // Cat again, in the same second: graded from where this review left it, not as the index
    // has it (new still), and logged a second later than its last grade.
    assert_eq!(review.current().unwrap().front, "cat");
    review.show_answer();
    let asked_index = std::cell::Cell::new(false);
    let second = review.grade(&settings, NOW, Grade::Good, |_| {
        asked_index.set(true);
        None
    });
    let second = second.unwrap();
    assert!(!asked_index.get(), "the review knows where it stands");
    assert_eq!((second.at, second.before), (NOW + 1, Some(first.after)));
    assert_ne!(second.after.step, first.after.step, "a step on, not the first again");
    // A card the index knows is graded from there.
    let known = CardState { stability: 10.0, difficulty: 5.0, last: NOW - 86_400 * 10, step: None, relearning: false };
    let mut other = ReviewSession::start("en", due("a.md", NOTE), Vec::new());
    other.next(read);
    other.show_answer();
    let graded = other.grade(&settings, NOW, Grade::Good, |_| Some(known)).unwrap();
    assert!(graded.before.is_some_and(|before| before.last == known.last) && graded.after.due > NOW + 86_400 * 10, "{graded:?}");
}

#[test]
fn the_last_grade_taken_back_shows_its_card_again_as_if_it_had_not_been_graded() {
    let settings = DeckSettings::default();
    let read = |_: &str| Some(NOTE.to_owned());
    let mut review = ReviewSession::start("en", due("a.md", NOTE), Vec::new());
    assert!(review.last_grade().is_none());
    review.take_back();
    review.next(read);
    review.show_answer();
    let graded = review.grade(&settings, NOW, Grade::Good, new).unwrap();
    review.graded(graded.clone());
    review.next(read);
    assert_eq!(review.last_grade().map(|(at, card)| (at, card.id.clone())), Some((NOW, Some("c-aaa111".into()))));
    assert_eq!((review.current().unwrap().front.as_str(), review.left(), review.reviewed()), ("dog", 3, 1));
    review.take_back();
    // Cat shows with its answer; dog waits first again; cat's place at the end is gone.
    let asked = review.current().unwrap();
    assert_eq!((asked.front.as_str(), asked.answered, review.left(), review.reviewed(), review.can_undo()), ("cat", true, 3, 0, false));
    // Graded as meant, from where it stood before: a new card again, in the same second.
    let again = review.grade(&settings, NOW, Grade::Good, new).unwrap();
    assert_eq!(again, graded);
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    review.next(read);
    assert_eq!((review.current().unwrap().front.as_str(), review.left()), ("dog", 2), "Easy: cat does not come back");
    // A grade taken back on the review's last card: the review is not done.
    review.show_answer();
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    review.next(read);
    review.show_answer();
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    review.next(read);
    assert!(review.current().is_none());
    review.take_back();
    assert_eq!((review.current().unwrap().front.as_str(), review.left(), review.reviewed()), ("cow", 1, 2));
    // A card graded twice, the second taken back: it stands as the first left it.
    let mut review = ReviewSession::start("en", due("a.md", "#@en cat::кіт ^c-aaa111\n"), Vec::new());
    let read = |_: &str| Some("#@en cat::кіт ^c-aaa111\n".to_owned());
    review.next(read);
    review.show_answer();
    let first = review.grade(&settings, NOW, Grade::Good, new).unwrap();
    review.graded(first.clone());
    review.next(read);
    review.show_answer();
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    review.next(read);
    review.take_back();
    assert_eq!(review.grade(&settings, NOW, Grade::Good, new).unwrap().before, Some(first.after));
}

#[test]
fn a_card_given_its_id_moves_those_waiting_after_it_in_its_note() {
    let text = "#@en cat::кіт\n#@en dog::пес\n";
    let mut cards = due("a.md", text);
    cards.extend(due("b.md", text));
    let mut review = ReviewSession::start("en", cards, Vec::new());
    review.next(|_| Some(text.to_owned()));
    review.show_answer();
    let settings = DeckSettings::default();
    assert!(review.grade(&settings, NOW, Grade::Good, new).is_none(), "no id, no grade: it is given one first");
    // ` ^c-aaa111` written at the first card's end.
    let (at, mark) = (review.current().unwrap().card.end as usize, " ^c-aaa111");
    let written = format!("{}{mark}{}", &text[..at], &text[at..]);
    review.id_given("c-aaa111".into(), at, mark.len());
    let shown = &review.current().unwrap().card;
    assert_eq!((shown.id.as_deref(), shown.end as usize), (Some("c-aaa111"), at + mark.len()));
    assert_eq!(review.grade(&settings, NOW, Grade::Easy, new).unwrap().id, "c-aaa111");
    review.graded(review.grade(&settings, NOW, Grade::Easy, new).unwrap());
    // The dog of a.md is found where it stands now; the cards of b.md where they were.
    let read = |path: &str| Some(if path == "a.md" { written.clone() } else { text.to_owned() });
    review.next(read);
    assert_eq!((review.current().unwrap().front.as_str(), review.current().unwrap().card.path.as_str()), ("dog", "a.md"));
    review.next(read);
    assert_eq!((review.current().unwrap().front.as_str(), review.current().unwrap().card.path.as_str()), ("cat", "b.md"));
}

#[test]
fn a_card_is_found_in_its_note_by_its_id_or_by_its_place() {
    let text = "#@en cat::кіт ^c-aaa111\n#@en cat::кицька ^c-aaa111\n#@en dog::пес\n";
    let cards = due("a.md", text);
    // Two with one id: each where the index found it; moved, the first of them.
    assert_eq!(card_written(text, &cards[1]).unwrap().back, "кицька");
    assert_eq!(card_written(&format!("\n{text}"), &cards[1]).unwrap().back, "кіт");
    // With no id: where it was, or not at all.
    assert_eq!(card_written(text, &cards[2]).unwrap().front, "dog");
    assert!(card_written(&format!("\n{text}"), &cards[2]).is_none());
    assert!(card_written("no cards here", &cards[0]).is_none());
}
