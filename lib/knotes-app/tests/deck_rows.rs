use knotes_app::deck_rows::{cards_ahead, deck_counts, due_cards, introduced};
use knotes_core::{CardRow, CardState, DeckCount, Reviewed};

/// A card of `deck` in `path`, never reviewed, or first reviewed at `first` and due at `due`.
fn card(path: &str, deck: &str, id: &str, reviewed: Option<(i64, i64)>) -> CardRow {
    let reviewed = reviewed.map(|(first, due)| Reviewed { state: CardState { stability: 3.0, difficulty: 5.0, last: first, step: None, relearning: false }, first, due, reviews: 1, lapses: 0 });
    CardRow { path: path.into(), start: 0, end: 1, deck: deck.into(), id: Some(id.into()), reviewed }
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
    let count = |deck: &str, cards, due, fresh| DeckCount { deck: deck.into(), cards, due, fresh };
    assert_eq!(deck_counts(&rows, 1000), [count("мови", 2, 0, 1), count("мови/англійська", 2, 1, 1), count("мови/німецька", 1, 1, 0), count("мовиці", 1, 1, 0)]);
    assert_eq!(introduced(&rows, 900), [("мови".to_owned(), 1), ("мови/німецька".to_owned(), 1)]);
    // The longest due first, then the new in the notes' order; a deck whose name only starts so is another.
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 10, 10)), ["c-5", "c-1", "c-2", "c-6"]);
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 10, 1)), ["c-5", "c-1", "c-2"], "the day's new cards");
    assert_eq!(ids(due_cards(&rows, "мови", 1000, 1, 10)), ["c-5"], "the limit of all");
    assert_eq!(ids(due_cards(&rows, "мови/англійська", 1000, 10, 0)), ["c-1"]);
    assert_eq!(ids(cards_ahead(&rows, "мови", 1000, 10)), ["c-3"]);
    assert_eq!(ids(cards_ahead(&rows, "мови", 350, 10)), ["c-5", "c-1", "c-3"], "the soonest due first");
    assert!(cards_ahead(&rows, "історія", 0, 10).is_empty() && deck_counts(&[], 0).is_empty());
}
