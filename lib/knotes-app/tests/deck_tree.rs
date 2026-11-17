use knotes_app::deck_tree;
use knotes_core::DeckCount;

fn deck(deck: &str, due: usize, fresh: usize) -> DeckCount {
    DeckCount { deck: deck.into(), cards: due + fresh, due, fresh }
}

#[test]
fn a_deck_asks_its_due_cards_and_the_new_ones_its_day_still_takes() {
    let decks = [deck("мови/англійська", 2, 30), deck("мови/німецька", 1, 5), deck("історія", 0, 3)];
    // мови asks 10 new a day, 4 of them taken today (in англійська); німецька none at all.
    let new_a_day = |deck: &str| match deck {
        "мови" | "мови/англійська" => 10,
        "мови/німецька" => 0,
        _ => 20,
    };
    let tree = deck_tree(&decks, &[("мови/англійська".to_owned(), 4), ("давня".to_owned(), 7)], &new_a_day);
    let shown: Vec<(&str, usize)> = tree.iter().flat_map(|n| std::iter::once(n).chain(&n.children)).map(|n| (n.tag.as_str(), n.notes)).collect();
    assert_eq!(shown, [("історія", 3), ("мови", 3 + 6), ("мови/англійська", 2 + 6), ("мови/німецька", 1)], "a deck gone counts nowhere");
}
