use knotes_cards::{DeckSetting, DeckSettings, NEW_A_DAY, RETENTION};

#[test]
fn a_deck_takes_what_the_nearest_deck_above_sets() {
    let settings = DeckSettings::parse(r#"{"мови": {"retention": 0.85, "new_a_day": 10}, "мови/англійська/слова": {"new_a_day": 0}}"#);
    assert_eq!((settings.retention("мови"), settings.new_a_day("мови")), (0.85, 10));
    assert_eq!((settings.retention("мови/англійська"), settings.new_a_day("мови/англійська")), (0.85, 10), "from above");
    assert_eq!((settings.retention("мови/англійська/слова"), settings.new_a_day("мови/англійська/слова")), (0.85, 0), "its own, the rest from above");
    assert_eq!((settings.retention("мовиці"), settings.new_a_day("історія")), (RETENTION, NEW_A_DAY), "no deck says: the defaults");
    assert_eq!(settings.inherited("мови/англійська"), (0.85, 10, vec![1, 10], None));
    assert_eq!(settings.inherited("мови"), (RETENTION, NEW_A_DAY, vec![1, 10], None));
    assert_eq!(settings.own("мови/англійська"), DeckSetting::default());
}

#[test]
fn settings_are_written_as_read_and_a_damaged_file_sets_nothing() {
    let mut settings = DeckSettings::parse(r#"{"a": {"retention": 0.99, "later": true}, "b": 5, "c": {"retention": "x"}}"#);
    assert_eq!(settings.retention("a"), 0.97, "brought into range");
    settings.set("b/c", DeckSetting { retention: None, new_a_day: Some(5), steps: None, reviews_a_day: None });
    settings.set("a", DeckSetting { retention: Some(0.8), new_a_day: None, steps: None, reviews_a_day: None });
    let text = settings.to_text();
    assert_eq!(text, "{\n  \"a\": {\n    \"later\": true,\n    \"retention\": 0.8\n  },\n  \"b/c\": {\n    \"new_a_day\": 5\n  }\n}\n", "another's key kept, a deck setting nothing left out");
    assert_eq!(DeckSettings::parse(&text).own("b/c"), DeckSetting { retention: None, new_a_day: Some(5), steps: None, reviews_a_day: None });
    for damaged in ["", "[]", "{", "null"] {
        assert_eq!(DeckSettings::parse(damaged), DeckSettings::default(), "{damaged:?}");
    }
    assert_eq!(DeckSettings::default().to_text(), "{}\n");
}

#[test]
fn a_deck_s_steps_are_its_own_the_nearest_above_or_the_vault_s() {
    let settings = DeckSettings::parse(r#"{"": {"steps": [5], "new_a_day": 7}, "мови": {"steps": []}, "мови/англійська": {"steps": [2, 0, 15, 9999]}}"#);
    assert_eq!(settings.step_minutes("історія"), [5], "the vault's own, above every deck");
    assert_eq!((settings.new_a_day("історія/давня"), settings.retention("історія")), (7, RETENTION));
    assert!(settings.step_minutes("мови").is_empty() && settings.steps("мови/німецька").learning.is_empty(), "none: a new card's days begin at once");
    assert_eq!(settings.step_minutes("мови/англійська"), [2, 15, 1440], "no step of nothing, none longer than a day");
    assert_eq!(settings.steps("мови/англійська"), knotes_cards::Steps { learning: vec![120, 900, 86_400], relearning: vec![600] });
    assert_eq!(DeckSettings::default().step_minutes("x"), [1, 10]);
    let mut set = DeckSettings::default();
    set.set("a", DeckSetting { retention: None, new_a_day: None, steps: Some(vec![]), reviews_a_day: None });
    assert_eq!(set.to_text(), "{\n  \"a\": {\n    \"steps\": []\n  }\n}\n");
    assert_eq!(DeckSettings::parse(&set.to_text()).own("a").steps, Some(vec![]));
}

#[test]
fn a_damaged_file_is_told_from_no_file_when_it_is_to_be_written_back() {
    assert_eq!(DeckSettings::read(""), Some(DeckSettings::default()), "no file yet");
    assert_eq!(DeckSettings::read("  \n"), Some(DeckSettings::default()));
    assert!(DeckSettings::read(r#"{"мови": {"new_a_day": 5}}"#).is_some_and(|s| s.new_a_day("мови") == 5));
    for damaged in ["{", r#"{"мови": {"new_a_day": 5},}"#, "[]", "null", "<<<<<<< HEAD"] {
        assert_eq!(DeckSettings::read(damaged), None, "{damaged:?}: not to be written over");
        assert_eq!(DeckSettings::parse(damaged), DeckSettings::default(), "the schedule goes on by the defaults");
    }
}

#[test]
fn a_deck_s_reviews_a_day_are_its_own_or_the_deck_s_above_and_none_while_no_deck_says() {
    let mut settings = DeckSettings::parse(r#"{"мови": {"reviews_a_day": 100}, "мови/німецька": {"reviews_a_day": 0}}"#);
    assert_eq!((settings.reviews_a_day("мови"), settings.reviews_a_day("мови/англійська/слова"), settings.reviews_a_day("мови/німецька")), (Some(100), Some(100), Some(0)));
    assert_eq!(settings.reviews_a_day("історія"), None, "no deck says: all that are due are asked");
    assert_eq!(settings.inherited("мови/англійська").3, Some(100));
    // Written back with the rest, and read the same.
    settings.set("історія", DeckSetting { retention: None, new_a_day: None, steps: None, reviews_a_day: Some(30) });
    let again = DeckSettings::parse(&settings.to_text());
    assert_eq!((again.reviews_a_day("історія"), again.reviews_a_day("мови")), (Some(30), Some(100)));
    assert!(settings.to_text().contains("\"reviews_a_day\": 30"), "{}", settings.to_text());
}

