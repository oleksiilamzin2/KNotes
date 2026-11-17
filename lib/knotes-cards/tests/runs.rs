use knotes_cards::{CardLink, Run, text_runs};

fn plain(text: &str) -> Run {
    Run { text: text.into(), link: None }
}

fn note(text: &str, target: &str) -> Run {
    Run { text: text.into(), link: Some(CardLink::Note(target.into())) }
}

#[test]
fn links_in_a_card_s_text_show_as_in_the_note_and_lead_where_they_say() {
    assert_eq!(text_runs("Див. [[Конотоп]] і [[Битви/Жовті Води#Перебіг|Жовті Води]]."), [plain("Див. "), note("Конотоп", "Конотоп"), plain(" і "), note("Жовті Води", "Битви/Жовті Води#Перебіг"), plain(".")]);
    assert_eq!(
        text_runs("[сайт](https://example.com/a?b=1), [лист](mailto:a@b.c), [нотатка](Папка/Моя%20нотатка.md)"),
        [
            Run { text: "сайт".into(), link: Some(CardLink::Web("https://example.com/a?b=1".into())) },
            plain(", "),
            Run { text: "лист".into(), link: Some(CardLink::Web("mailto:a@b.c".into())) },
            plain(", "),
            note("нотатка", "Папка/Моя нотатка"),
        ]
    );
    assert_eq!(text_runs("Схема: ![[схема.png]]"), [plain("Схема: "), note("схема.png", "схема.png")], "an embed's ! goes with it");
}

#[test]
fn what_is_no_link_stays_text() {
    for text in ["просто текст", "[[]] і [[не закрито", "[текст] (не посилання)", "[a](дві частини)", "масив[0] = (1)", "рядок\nдругий"] {
        assert_eq!(text_runs(text), [plain(text)], "{text}");
    }
    assert!(text_runs("").is_empty());
}
