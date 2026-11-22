//! The calendar of daily notes (the right panel): a note named by its day, `2026-11-22.md`, is
//! that day's note wherever it lies; a month is shown in weeks from Monday.

use std::collections::HashMap;

use serde_json::Value;

use crate::{FolderRules, days_from_civil};

/// A day: its year, month (1–12) and day of the month.
pub type Day = (i32, u8, u8);

/// The day the note at `path` is named by; none for another name.
pub fn daily_day(path: &str) -> Option<Day> {
    let stem = path.rsplit('/').next()?.strip_suffix(".md")?;
    let bytes = stem.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' || !bytes.iter().enumerate().all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit()) {
        return None;
    }
    let (year, month, day) = (stem[..4].parse().ok()?, stem[5..7].parse().ok()?, stem[8..].parse().ok()?);
    ((1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day)).then_some((year, month, day))
}

/// The name of `day`'s note: `2026-11-22`.
pub fn daily_name((year, month, day): Day) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Where `day`'s note is made: in the folder the root's rules name (`"daily": {"folder"}`),
/// else `default`; `""` is the vault's root.
pub fn daily_note_path(rules: &HashMap<String, FolderRules>, default: &str, day: Day) -> String {
    let set = rules.get("").and_then(|r| r.object().get("daily")).and_then(|d| d.get("folder")).and_then(Value::as_str);
    let folder = set.unwrap_or(default).trim_matches('/');
    if folder.is_empty() { format!("{}.md", daily_name(day)) } else { format!("{folder}/{}.md", daily_name(day)) }
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// The days of a month as its weeks show them, from Monday: none before its first day and
/// after its last, whole weeks.
pub fn month_cells(year: i32, month: u8) -> Vec<Option<u8>> {
    // 1970-01-01 was a Thursday.
    let before = (days_from_civil(year, month, 1) + 3).rem_euclid(7) as usize;
    let mut cells: Vec<Option<u8>> = vec![None; before];
    cells.extend((1..=days_in_month(year, month)).map(Some));
    cells.resize(cells.len().div_ceil(7) * 7, None);
    cells
}

/// The month `by` months after (before, when negative) `month` of `year`.
pub fn month_after(year: i32, month: u8, by: i32) -> (i32, u8) {
    let months = year * 12 + i32::from(month) - 1 + by;
    (months.div_euclid(12), (months.rem_euclid(12) + 1) as u8)
}

/// The day it is `seconds` after 1970 began, `utc_offset_s` east of Greenwich (Howard
/// Hinnant's civil-from-days).
pub fn day_at(seconds: i64, utc_offset_s: i32) -> Day {
    let z = (seconds + i64::from(utc_offset_s)).div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (day, month) = ((doy - (153 * mp + 2) / 5 + 1) as u8, if mp < 10 { mp + 3 } else { mp - 9 } as u8);
    ((yoe + era * 400 + i64::from(month <= 2)) as i32, month, day)
}
