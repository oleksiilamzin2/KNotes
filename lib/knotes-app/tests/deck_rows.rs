use knotes_app::deck_rows::{cards_ahead, deck_counts, due_cards, introduced};
use knotes_core::{CardRow, CardState, DeckCount, Reviewed};

/// A card of `deck` in `path`, never reviewed, or first reviewed at `first` and due at `due`.
fn card(path: &str, deck: &str, id: &str, reviewed: Option<(i64, i64)>) -> CardRow {
    let reviewed = reviewed.map(|(first, due)| Reviewed { state: CardState { stability: 3.0, difficulty: 5.0, last: first, step: None, relearning: false }, first, due, reviews: 1, lapses: 0 });
    CardRow { path: path.into(), start: 0, end: 1, deck: deck.into(), id: Some(id.into()), reviewed, paused: false }
}

fn ids(cards: Vec<knotes_core::DueCard>) -> Vec<String> {
    cards.into_iter().map(|c| c.id.unwrap()).collect()
}

#[test]
fn a_part_of_the_cards_is_counted_and_queued_as_the_index_does_all() {
    let rows = [
        card("a.md", "мови/англійська", "c-1", Some((100, 500))),
        card("a.md", "мови/англійська", "c-2", None),
        card("a.md", "мови", "c-3", Some((900, 2000))),
        card("b.md", "мовиці", "c-4", Some((100, 300))),
        card("b.md", "мови/німецька", "c-5", Some((950, 400))),
        card("b.md", "мови", "c-6", None),
    ];
    let count = |deck: &str, cards, due, fresh| DeckCount { deck: deck.into(), cards, due, fresh, learning: 0 };
    assert_eq!(deck_counts(&rows, 1000), [count("мови", 2, 0, 1), count("мови/англійська", 2, 1, 1), count("мови/німецька", 1, 1, 0), count("мовиці", 1, 1, 0)]);
    assert_eq!(introduced(&rows, 900), [("мови".to_owned(), 1), ("мови/німецька".to_owned(), 1)]);
    // The longest due first, then the new in the notes' order; a deck whose name only starts so is another.
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 10, 10, usize::MAX)), ["c-5", "c-1", "c-2", "c-6"]);
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 10, 1, usize::MAX)), ["c-5", "c-1", "c-2"], "the day's new cards");
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 1, 10, usize::MAX)), ["c-5"], "the limit of all");
    assert_eq!(ids(due_cards(&rows, "мови/англійська", 1000, 10, 0, usize::MAX)), ["c-1"]);
    assert_eq!(ids(cards_ahead(&rows, "мови", 1000, 10)), ["c-3"]);
    assert_eq!(ids(cards_ahead(&rows, "мови", 350, 10)), ["c-5", "c-1", "c-3"], "the soonest due first");
    assert!(cards_ahead(&rows, "історія", 0, 10).is_empty() && deck_counts(&[], 0).is_empty());
}

#[test]
fn a_card_paused_is_counted_in_its_deck_but_does_not_wait() {
    let mut rows = vec![card("a.md", "en", "due", Some((0, 50))), card("a.md", "en", "ahead", Some((0, 500))), card("a.md", "en", "new", None)];
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_default()).collect::<Vec<_>>().join(" ");
    assert_eq!((ids(due_cards(&rows, "en", 100, 10, 10, usize::MAX)), ids(cards_ahead(&rows, "en", 100, 10))), ("due new".to_owned(), "ahead".to_owned()));
    for row in &mut rows {
        row.paused = true;
    }
    let counts = deck_counts(&rows, 100);
    assert_eq!((counts[0].cards, counts[0].due, counts[0].fresh), (3, 0, 0));
    assert_eq!((ids(due_cards(&rows, "en", 100, 10, 10, usize::MAX)), ids(cards_ahead(&rows, "en", 100, 10))), (String::new(), String::new()));
}

#[test]
fn a_part_s_due_cards_are_cut_to_the_day_s_reviews_but_for_those_at_a_step() {
    use knotes_app::deck_rows::reviewed;
    let step = |mut row: CardRow| {
        if let Some(s) = row.reviewed.as_mut() {
            s.state.step = Some(0);
        }
        row
    };
    // Three due whose days have begun (the longest due first: old, older... by their due), one
    // due at a step, one new.
    let rows = vec![card("a.md", "en", "a", Some((0, 30))), card("a.md", "en", "b", Some((0, 10))), card("a.md", "en", "c", Some((0, 20))), step(card("a.md", "en", "s", Some((0, 25)))), card("a.md", "en", "n", None)];
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_default()).collect::<Vec<_>>().join(" ");
    assert_eq!(ids(due_cards(&rows, "en", 100, 10, 10, usize::MAX)), "b c s a n");
    assert_eq!(ids(due_cards(&rows, "en", 100, 10, 10, 2)), "b c s n", "two of the three, the longest due; the one at a step besides");
    assert_eq!(ids(due_cards(&rows, "en", 100, 10, 0, 0)), "s");
    let counts = deck_counts(&rows, 100);
    assert_eq!((counts[0].due, counts[0].learning, counts[0].fresh), (4, 1, 1));
    // Today's reviews: last reviewed since the day began, first reviewed before it, at no step.
    let mut done = card("a.md", "en", "d", Some((0, 500)));
    done.reviewed.as_mut().unwrap().state.last = 60;
    let mut learned_today = card("b.md", "en", "t", Some((60, 500)));
    learned_today.reviewed.as_mut().unwrap().state.last = 70;
    let mut again = step(card("b.md", "de", "g", Some((0, 80))));
    again.reviewed.as_mut().unwrap().state.last = 70;
    assert_eq!(reviewed(&[done, learned_today, again, rows[0].clone()], 50), [("en".to_owned(), 1)]);
}

