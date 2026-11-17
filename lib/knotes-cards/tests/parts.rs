use knotes_cards::card_parts;

/// The parts of the only card of `text`, as the text they cover: its deck's mark, its question, its marks.
fn parts(text: &str) -> (String, Option<String>, Vec<String>) {
    let all = card_parts(text);
    assert_eq!(all.len(), 1, "{text:?}");
    let part = &all[0];
    (text[part.deck.clone()].to_owned(), part.question.clone().map(|q| text[q].to_owned()), part.marks.iter().map(|m| text[m.clone()].to_owned()).collect())
}

#[test]
fn a_card_s_parts_are_found_where_it_is_written() {
    let q = |text: &str| Some(text.to_owned());
    assert_eq!(parts("Вступ.\n\n#@мови/англійська cat  ::  кіт ^c-1\n"), ("#@мови/англійська".into(), q("cat"), vec!["::".to_owned()]));
    assert_eq!(parts("- #@мови Що це?\n  ?\n  Відповідь. ^c-2\n"), ("#@мови".into(), q("Що це?"), vec!["?".to_owned()]), "a list item's, the answer under a `?` line");
    assert_eq!(parts("#@історія Коли була\nбитва?\n?\n1659 року: взимку."), ("#@історія".into(), q("Коли була\nбитва?"), vec!["?".to_owned()]), "the `?` line parts it, not a colon after");
    assert_eq!(parts("#@мови{\nПерший абзац.\n\nДругий.\n?\nВідповідь.\n} ^c-3\n"), ("#@мови{".into(), q("Перший абзац.\n\nДругий."), vec!["?".to_owned(), "}".to_owned()]));
    assert_eq!(parts("#@мови Кіт — це ==тварина==. ^c-4"), ("#@мови".into(), None, vec![]), "a cloze has no question apart");
    assert_eq!(parts("#@мови{\nКіт — це ==тварина==.\n}"), ("#@мови{".into(), None, vec!["}".to_owned()]));
}

#[test]
fn what_is_no_card_has_no_parts() {
    assert!(card_parts("#@мови лише питання\n\n```\n#@код a::b\n```\n\nзвичайний текст :: з двокрапками").is_empty());
    let two = card_parts("#@a x::y\n#@b z::w ^c-9\n");
    assert_eq!(two.len(), 2);
    assert!(two[0].deck.end < two[1].deck.start);
}
