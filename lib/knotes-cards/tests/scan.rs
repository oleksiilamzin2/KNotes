use knotes_cards::{Card, CardKind, scan_cards};

fn of(text: &str) -> Vec<(String, Option<String>, CardKind, String, String)> {
    scan_cards(text).into_iter().map(|c: Card| (c.deck, c.id, c.kind, c.front, c.back)).collect()
}

#[test]
fn a_card_on_a_line_has_its_deck_its_question_its_answer_and_its_id() {
    let text = "Вступ.\n\n#@мови/англійська to procrastinate::зволікати ^c-a1b2c3\n\nДалі текст.";
    let cards = scan_cards(text);
    assert_eq!(of(text), [("мови/англійська".to_owned(), Some("c-a1b2c3".to_owned()), CardKind::Basic, "to procrastinate".to_owned(), "зволікати".to_owned())]);
    assert_eq!(&text[cards[0].span.clone()], "#@мови/англійська to procrastinate::зволікати ^c-a1b2c3", "from its deck to its id");
}

#[test]
fn a_card_over_several_lines_parts_at_the_question_mark_line() {
    let text = "#@історія Коли була\nбитва під Конотопом?\n?\n1659 року.\nПеремога Виговського. ^c-k1\n\nне картка";
    assert_eq!(of(text), [("історія".to_owned(), Some("c-k1".to_owned()), CardKind::Basic, "Коли була\nбитва під Конотопом?".to_owned(), "1659 року.\nПеремога Виговського.".to_owned())]);
}

#[test]
fn a_cloze_leaves_its_marked_words_out() {
    let text = "#@географія Столиця ==Австралії== — ==Канберра==.";
    assert_eq!(of(text), [("географія".to_owned(), None, CardKind::Cloze, "Столиця […] — […].".to_owned(), "Столиця Австралії — Канберра.".to_owned())]);
}

#[test]
fn cards_in_a_list_and_one_under_another_are_each_their_own() {
    let text = "- #@a q1::a1\n- #@a/b q2::a2 ^c-2\n#@c q3::a3\n1. #@d q4::a4";
    let decks: Vec<String> = scan_cards(text).into_iter().map(|c| c.deck).collect();
    assert_eq!(decks, ["a", "a/b", "c", "d"]);
}

#[test]
fn code_the_frontmatter_and_text_that_asks_nothing_hold_no_cards() {
    let text = "---\ntitle: x\nnote: \"#@a q::a\"\n---\n```\n#@a in::code\n```\n\n#@ no deck::x\n\n#@a лише текст без питання\n\nтекст #@a посеред::рядка\n\n#@a ::без питання\n\n#@справжня так::ні";
    assert_eq!(of(text), [("справжня".to_owned(), None, CardKind::Basic, "так".to_owned(), "ні".to_owned())]);
}

#[test]
fn an_id_is_only_one_at_the_end_after_a_space() {
    assert_eq!(scan_cards("#@a q::a^c-1")[0].id, None, "no space before it");
    assert_eq!(scan_cards("#@a q::a ^c-1 і ще")[0].id, None, "not at the end");
    let card = &scan_cards("#@a q\n?\na\n^c-9z")[0];
    assert_eq!((card.id.as_deref(), card.back.as_str()), (Some("c-9z"), "a"), "on a line of its own");
}
