use std::collections::HashMap;

use knotes_app::{FolderRules, daily_day, daily_name, daily_note_path, day_at, days_in_month, month_after, month_cells};

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
