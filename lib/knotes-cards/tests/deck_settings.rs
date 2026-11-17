use knotes_cards::{DeckSetting, DeckSettings, NEW_A_DAY, RETENTION};

#[test]
fn a_deck_takes_what_the_nearest_deck_above_sets() {
    let settings = DeckSettings::parse(r#"{"мови": {"retention": 0.85, "new_a_day": 10}, "мови/англійська/слова": {"new_a_day": 0}}"#);
    assert_eq!((settings.retention("мови"), settings.new_a_day("мови")), (0.85, 10));
    assert_eq!((settings.retention("мови/англійська"), settings.new_a_day("мови/англійська")), (0.85, 10), "from above");
    assert_eq!((settings.retention("мови/англійська/слова"), settings.new_a_day("мови/англійська/слова")), (0.85, 0), "its own, the rest from above");
    assert_eq!((settings.retention("мовиці"), settings.new_a_day("історія")), (RETENTION, NEW_A_DAY), "no deck says: the defaults");
    assert_eq!(settings.inherited("мови/англійська"), (0.85, 10));
    assert_eq!(settings.inherited("мови"), (RETENTION, NEW_A_DAY));
    assert_eq!(settings.own("мови/англійська"), DeckSetting::default());
}

#[test]
fn settings_are_written_as_read_and_a_damaged_file_sets_nothing() {
    let mut settings = DeckSettings::parse(r#"{"a": {"retention": 0.99, "later": true}, "b": 5, "c": {"retention": "x"}}"#);
    assert_eq!(settings.retention("a"), 0.97, "brought into range");
    settings.set("b/c", DeckSetting { retention: None, new_a_day: Some(5) });
    settings.set("a", DeckSetting { retention: Some(0.8), new_a_day: None });
    let text = settings.to_text();
    assert_eq!(text, "{\n  \"a\": {\n    \"later\": true,\n    \"retention\": 0.8\n  },\n  \"b/c\": {\n    \"new_a_day\": 5\n  }\n}\n", "another's key kept, a deck setting nothing left out");
    assert_eq!(DeckSettings::parse(&text).own("b/c"), DeckSetting { retention: None, new_a_day: Some(5) });
    for damaged in ["", "[]", "{", "null"] {
        assert_eq!(DeckSettings::parse(damaged), DeckSettings::default(), "{damaged:?}");
    }
    assert_eq!(DeckSettings::default().to_text(), "{}\n");
}
