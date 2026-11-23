//! The calendar of daily notes (the right panel): a note named by its day, `2026-11-22.md`, is
//! that day's note wherever it lies; a month is shown in weeks from Monday.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{FolderRules, Moves, days_from_civil};

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

/// How the daily notes are laid out in their folder: all in it, or in folders by the year, by
/// the year and the month, or by the year, the month and the week (each a number).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DailyLayout {
    #[default]
    Flat,
    Year,
    Month,
    Week,
}

impl DailyLayout {
    pub const ALL: [Self; 4] = [Self::Flat, Self::Year, Self::Month, Self::Week];

    pub fn key(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Year => "year",
            Self::Month => "month",
            Self::Week => "week",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|layout| layout.key() == key)
    }

    /// The folders `day`'s note lies in, under the folder of daily notes: `2026/11/48`.
    fn folders(self, (year, month, day): Day) -> String {
        match self {
            Self::Flat => String::new(),
            Self::Year => format!("{year:04}"),
            Self::Month => format!("{year:04}/{month:02}"),
            Self::Week => format!("{year:04}/{month:02}/{:02}", iso_week((year, month, day))),
        }
    }
}

impl FolderRules {
    /// The folder of daily notes this (root) file names (`"daily": {"folder"}`); `""` is the
    /// vault's root.
    pub fn daily_folder(&self) -> Option<&str> {
        self.object().get("daily").and_then(|d| d.get("folder")).and_then(Value::as_str).map(|folder| folder.trim_matches('/'))
    }

    /// How this (root) file lays the daily notes out (`"daily": {"layout"}`).
    pub fn daily_layout(&self) -> DailyLayout {
        self.object().get("daily").and_then(|d| d.get("layout")).and_then(Value::as_str).and_then(DailyLayout::from_key).unwrap_or_default()
    }
}

/// The root rules file's `text` with the daily notes' `folder` and `layout` in it, its other
/// keys kept. An empty text is a file not there yet.
pub fn with_daily(text: &str, folder: &str, layout: DailyLayout) -> Result<String, String> {
    let mut root = FolderRules::parse(if text.trim().is_empty() { "{}" } else { text })?.object().clone();
    let daily = root.entry("daily").or_insert_with(|| json!({}));
    if !daily.is_object() {
        *daily = json!({});
    }
    if let Some(daily) = daily.as_object_mut() {
        daily.insert("folder".into(), json!(folder.trim_matches('/')));
        daily.insert("layout".into(), json!(layout.key()));
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Where `day`'s note is made: in the folder the root's rules name (`"daily": {"folder"}`),
/// else `default` (`""` is the vault's root), in the folders its layout asks for.
pub fn daily_note_path(rules: &HashMap<String, FolderRules>, default: &str, day: Day) -> String {
    let root = rules.get("");
    let folder = root.and_then(FolderRules::daily_folder).unwrap_or(default.trim_matches('/'));
    let folders = root.map(FolderRules::daily_layout).unwrap_or_default().folders(day);
    let parts: Vec<&str> = [folder, folders.as_str()].into_iter().filter(|part| !part.is_empty()).collect();
    if parts.is_empty() { format!("{}.md", daily_name(day)) } else { format!("{}/{}.md", parts.join("/"), daily_name(day)) }
}

/// The daily notes among `paths` (every note of the vault) that do not lie where the rules lay
/// them out, each with where it goes, and how many stay because their place is taken: by a note
/// there already, or by another of the same day, of which the first by path goes.
pub fn daily_moves<'a>(paths: impl IntoIterator<Item = &'a str>, rules: &HashMap<String, FolderRules>, default: &str) -> (Moves, usize) {
    let mut paths: Vec<&str> = paths.into_iter().collect();
    paths.sort_unstable();
    let mut taken: std::collections::HashSet<String> = paths.iter().map(|path| (*path).to_owned()).collect();
    let (mut moves, mut staying) = (Moves::new(), 0);
    for path in paths {
        let Some(day) = daily_day(path) else { continue };
        let to = daily_note_path(rules, default, day);
        if to == path {
            continue;
        }
        if taken.insert(to.clone()) {
            moves.insert(path.to_owned(), to);
        } else {
            staying += 1;
        }
    }
    (moves, staying)
}

/// The week of the year `day` is in, as ISO 8601 counts them: weeks start on Monday, and the
/// first is the one with the year's first Thursday (so a year's first days may be in week 52
/// or 53, and its last in week 1).
pub fn iso_week((year, month, day): Day) -> u8 {
    let days = days_from_civil(year, month, day);
    let weekday = (days + 3).rem_euclid(7);
    let ordinal = days - days_from_civil(year, 1, 1) + 1;
    let week = (ordinal - weekday + 9) / 7;
    // A year has 53 weeks when it starts on a Thursday, or on a Wednesday and is a leap year.
    let weeks_in = |year: i32| {
        let first = (days_from_civil(year, 1, 1) + 3).rem_euclid(7);
        if first == 3 || (first == 2 && days_in_month(year, 2) == 29) { 53 } else { 52 }
    };
    match week {
        0 => weeks_in(year - 1),
        53 if weeks_in(year) == 52 => 1,
        week => week as u8,
    }
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
