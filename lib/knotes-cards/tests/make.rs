use knotes_cards::{asks, card_id, give_id, make_card, scan_cards};

fn made(text: &str, selected: &str, deck: &str, question: Option<&str>) -> Option<String> {
    let start = text.find(selected).unwrap();
    let (range, with) = make_card(text, start..start + selected.len(), deck, question, "c-abc123")?;
    Some(format!("{}{with}{}", &text[..range.start], &text[range.end..]))
}

#[test]
fn a_selection_becomes_the_answer_of_a_card_in_a_paragraph_of_its_own() {
    assert_eq!(made("Кіт — це тварина.", "тварина", "біологія", Some("Хто такий кіт?")).unwrap(), "Кіт — це \n\n#@біологія Хто такий кіт?::тварина ^c-abc123\n\n.");
    assert_eq!(made("Вступ.\n\n1659 року.\n\nДалі.", "1659 року.", "історія", Some("Коли?")).unwrap(), "Вступ.\n\n#@історія Коли?::1659 року. ^c-abc123\n\nДалі.", "blank lines there already");
    assert_eq!(made("рядок\nвідповідь\nще", "відповідь", "a", Some("q")).unwrap(), "рядок\n\n#@a q::відповідь ^c-abc123\n\nще");
    let long = made("Перший рядок.\n\nДругий рядок.", "Перший рядок.\n\nДругий рядок.", "a/b", Some("Що?")).unwrap();
    assert_eq!(long, "#@a/b Що?\n?\nПерший рядок.\nДругий рядок. ^c-abc123", "several lines: apart on a ? line, no blank line inside");
    let cards = scan_cards(&long);
    assert_eq!((cards[0].front.as_str(), cards[0].back.as_str(), cards[0].id.as_deref()), ("Що?", "Перший рядок.\nДругий рядок.", Some("c-abc123")));
}

#[test]
fn a_selection_that_asks_already_needs_no_question() {
    assert!(asks("cat::кіт") && asks("Столиця ==Австралії==") && asks("Коли?\n?\nТоді.") && !asks("просто текст"));
    assert_eq!(made("слова: cat::кіт і все", "cat::кіт", "/мови/", None).unwrap(), "слова: \n\n#@мови cat::кіт ^c-abc123\n\n і все");
    assert_eq!(made("Столиця ==Австралії==.", "Столиця ==Австралії==.", "гео", None).unwrap(), "#@гео Столиця ==Австралії==. ^c-abc123");
    assert_eq!(made("просто текст", "просто текст", "a", None), None, "no question, and it asks nothing");
    assert_eq!(made("текст", "текст", "  ", Some("q")), None, "no deck");
    assert_eq!(make_card("текст", 2..2, "a", Some("q"), "c-1"), None, "nothing selected");
}

#[test]
fn a_card_without_an_id_is_given_one_at_its_end() {
    let text = "#@a q::a\n\n#@b Довге\n?\nвідповідь\n\nкінець";
    let cards = scan_cards(text);
    let (at, id) = give_id(text, cards[1].span.clone(), "c-zz9").unwrap();
    let with = format!("{}{id}{}", &text[..at], &text[at..]);
    assert_eq!(with, "#@a q::a\n\n#@b Довге\n?\nвідповідь ^c-zz9\n\nкінець");
    assert_eq!(scan_cards(&with)[1].id.as_deref(), Some("c-zz9"));
    assert_eq!(give_id(&with, scan_cards(&with)[1].span.clone(), "c-new"), None, "it has one");
    assert_eq!(give_id(text, 3..9, "c-new"), None, "no card there");
}

#[test]
fn ids_are_short_and_differ() {
    let ids: std::collections::HashSet<String> = (0..10_000).map(card_id).collect();
    assert_eq!(ids.len(), 10_000);
    assert!(ids.iter().all(|id| id.len() == 8 && id.starts_with("c-") && id[2..].bytes().all(|b| b.is_ascii_alphanumeric())));
}
