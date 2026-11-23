use std::collections::HashMap;

use knotes_app::{DailyLayout, FolderRules, Moves, daily_day, daily_moves, daily_name, daily_note_path, day_at, days_in_month, iso_week, month_after, month_cells, with_daily};

#[test]
fn a_note_named_by_its_day_is_that_day_s_wherever_it_lies() {
    assert_eq!(daily_day("Щоденник/2026-11-22.md"), Some((2026, 11, 22)));
    assert_eq!(daily_day("2024-02-29.md"), Some((2024, 2, 29)), "a leap year's");
    for other in ["2026-11-22.pdf", "2026-11-22 notes.md", "2026-13-01.md", "2026-02-30.md", "2026-1-5.md", "день.md", "2026-11-22"] {
        assert_eq!(daily_day(other), None, "{other}");
    }
    assert_eq!(daily_name((2026, 3, 5)), "2026-03-05");
}

#[test]
fn a_day_s_note_is_made_in_the_folder_the_root_s_rules_name() {
    let rules = |json: &str| HashMap::from([(String::new(), FolderRules::parse(json).unwrap())]);
    assert_eq!(daily_note_path(&HashMap::new(), "Щоденник", (2026, 11, 22)), "Щоденник/2026-11-22.md");
    assert_eq!(daily_note_path(&rules(r#"{"daily": {"folder": "Journal/2026/"}}"#), "Щоденник", (2026, 11, 22)), "Journal/2026/2026-11-22.md");
    assert_eq!(daily_note_path(&rules(r#"{"daily": {"folder": ""}}"#), "Щоденник", (2026, 11, 22)), "2026-11-22.md", "the vault's root");
}

#[test]
fn a_month_is_shown_in_whole_weeks_from_monday() {
    // November 2026 starts on a Sunday and has 30 days.
    let cells = month_cells(2026, 11);
    assert_eq!((cells.len(), cells[5], cells[6], cells[7], cells[35], cells[36]), (42, None, Some(1), Some(2), Some(30), None));
    // February 2027 starts on a Monday: four weeks, none empty.
    assert_eq!(month_cells(2027, 2), (1..=28).map(Some).collect::<Vec<_>>());
    assert_eq!((days_in_month(2024, 2), days_in_month(2100, 2), days_in_month(2026, 12)), (29, 28, 31));
    assert_eq!((month_after(2026, 12, 1), month_after(2026, 1, -1), month_after(2026, 11, -23)), ((2027, 1), (2025, 12), (2024, 12)));
}

#[test]
fn the_day_it_is_follows_the_time_zone() {
    // 2026-11-22 23:30 in Greenwich.
    let seconds = 1_795_390_200;
    assert_eq!(day_at(seconds, 0), (2026, 11, 22));
    assert_eq!(day_at(seconds, 2 * 3600), (2026, 11, 23), "past midnight in Kyiv");
    assert_eq!(day_at(0, -3600), (1969, 12, 31));
}

#[test]
fn the_daily_notes_are_laid_out_by_the_year_the_month_and_the_week() {
    let rules = |layout: DailyLayout| HashMap::from([(String::new(), FolderRules::parse(&with_daily(r#"{"trash": {"days": 14}}"#, "/Journal/", layout).unwrap()).unwrap())]);
    let path = |layout: DailyLayout| daily_note_path(&rules(layout), "Щоденник", (2026, 3, 5));
    assert_eq!(path(DailyLayout::Flat), "Journal/2026-03-05.md");
    assert_eq!(path(DailyLayout::Year), "Journal/2026/2026-03-05.md");
    assert_eq!(path(DailyLayout::Month), "Journal/2026/03/2026-03-05.md");
    assert_eq!(path(DailyLayout::Week), "Journal/2026/03/10/2026-03-05.md");
    let written = rules(DailyLayout::Week);
    assert_eq!((written[""].daily_folder(), written[""].daily_layout(), written[""].trash_days()), (Some("Journal"), DailyLayout::Week, 14), "the other keys stay");
    // In the vault's root, laid out all the same; a layout not known is the flat one.
    let root = HashMap::from([(String::new(), FolderRules::parse(&with_daily("", "", DailyLayout::Year).unwrap()).unwrap())]);
    assert_eq!(daily_note_path(&root, "Щоденник", (2026, 3, 5)), "2026/2026-03-05.md");
    assert_eq!(FolderRules::parse(r#"{"daily": {"layout": "decade"}}"#).unwrap().daily_layout(), DailyLayout::Flat);
    assert!(with_daily("{broken", "Journal", DailyLayout::Year).is_err(), "a file that does not parse is left");
}

#[test]
fn weeks_are_counted_as_iso_8601_counts_them() {
    // 2026 starts on a Thursday: its first day is in week 1, and it has 53 weeks.
    assert_eq!((iso_week((2026, 1, 1)), iso_week((2026, 1, 4)), iso_week((2026, 1, 5))), (1, 1, 2));
    assert_eq!((iso_week((2026, 11, 22)), iso_week((2026, 11, 23)), iso_week((2026, 12, 31))), (47, 48, 53));
    // The first days of 2027 are still in 2026's last week; the last of 2024 in 2025's first.
    assert_eq!((iso_week((2027, 1, 3)), iso_week((2027, 1, 4)), iso_week((2024, 12, 30)), iso_week((2025, 12, 28))), (53, 1, 1, 52));
}

#[test]
fn the_daily_notes_not_where_the_layout_puts_them_are_moved_there() {
    let rules = HashMap::from([(String::new(), FolderRules::parse(&with_daily("", "Journal", DailyLayout::Month).unwrap()).unwrap())]);
    let paths = ["Old/2026-11-03.md", "2026-11-03.md", "Journal/2026/11/2026-11-21.md", "Journal/2026-10-31.md", "Journal/2026/10/2026-10-30.md", "Projects/Plan.md", "Old/2026-10-30.md"];
    let (moves, staying) = daily_moves(paths, &rules, "Щоденник");
    let expected = Moves::from([
        // Two notes of one day: the first by path goes, the other stays.
        ("2026-11-03.md".to_string(), "Journal/2026/11/2026-11-03.md".to_string()),
        ("Journal/2026-10-31.md".to_string(), "Journal/2026/10/2026-10-31.md".to_string()),
    ]);
    assert_eq!((moves, staying), (expected, 2), "one in its place stays, one whose place is taken too; other notes are not daily ones");
}
