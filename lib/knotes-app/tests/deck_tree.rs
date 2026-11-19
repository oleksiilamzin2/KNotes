use knotes_app::deck_tree;
use knotes_core::DeckCount;

fn deck(deck: &str, due: usize, fresh: usize) -> DeckCount {
    DeckCount { deck: deck.into(), cards: due + fresh, due, fresh, learning: 0 }
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
    let tree = deck_tree(&decks, &[("мови/англійська".to_owned(), 4), ("давня".to_owned(), 7)], &new_a_day, &[], &|_| None);
    let shown: Vec<(&str, usize)> = tree.iter().flat_map(|n| std::iter::once(n).chain(&n.children)).map(|n| (n.tag.as_str(), n.notes)).collect();
    assert_eq!(shown, [("історія", 3), ("мови", 3 + 6), ("мови/англійська", 2 + 6), ("мови/німецька", 1)], "a deck gone counts nowhere");
}

#[test]
fn a_deck_asks_no_more_of_its_due_cards_than_its_day_still_takes() {
    // мови/англійська: 30 due, 4 of them still at a step; 10 new. історія: 5 due.
    let decks = [DeckCount { deck: "мови/англійська".into(), cards: 40, due: 30, fresh: 10, learning: 4 }, deck("історія", 5, 0)];
    let new_a_day = |_: &str| 0;
    let asks = |reviewed: &[(String, usize)], a_day: &dyn Fn(&str) -> Option<usize>| {
        fn flat(nodes: &[knotes_app::TagNode], out: &mut Vec<String>) {
            for node in nodes {
                out.push(format!("{} {}", node.tag, node.notes));
                flat(&node.children, out);
            }
        }
        let mut out = Vec::new();
        flat(&deck_tree(&decks, &[], &new_a_day, reviewed, a_day), &mut out);
        out.join(", ")
    };
    assert_eq!(asks(&[], &|_| None), "історія 5, мови 30, мови/англійська 30", "no deck says how many a day: all that are due");
    // Ten a day: ten of the 26 whose days have begun, and the four at a step besides.
    let ten = |deck: &str| deck.starts_with("мови").then_some(10);
    assert_eq!(asks(&[], &ten), "історія 5, мови 14, мови/англійська 14");
    // Seven reviewed today already: three more; all ten: only those at a step.
    assert_eq!(asks(&[("мови/англійська".to_owned(), 7)], &ten), "історія 5, мови 7, мови/англійська 7");
    assert_eq!(asks(&[("мови/англійська".to_owned(), 12)], &ten), "історія 5, мови 4, мови/англійська 4");
    // None a day: the deck rests, but for what it began.
    assert_eq!(asks(&[], &|_| Some(0)), "історія 0, мови 4, мови/англійська 4");
    // A deck with no cards left has no reviews to count.
    assert_eq!(asks(&[("зникла".to_owned(), 9)], &ten), "історія 5, мови 14, мови/англійська 14");
}

