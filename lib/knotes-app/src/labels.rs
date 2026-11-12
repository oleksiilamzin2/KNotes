//! Choices behind user-visible labels; the UI turns them into localized text.

use crate::Clock;
use crate::note_list::local_day;

/// How a note row shows its modification time, as in Apple Notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeStyle {
    /// Today: the time of day.
    Time,
    Yesterday,
    /// Within the last week: the weekday name.
    Weekday,
    /// Older: the date.
    Date,
}

pub fn time_style(mtime_ns: i64, clock: &Clock) -> TimeStyle {
    match local_day(clock.now_ns, clock) - local_day(mtime_ns, clock) {
        i64::MIN..=0 => TimeStyle::Time,
        1 => TimeStyle::Yesterday,
        2..=6 => TimeStyle::Weekday,
        _ => TimeStyle::Date,
    }
}

/// Ukrainian plural form for `n`: `one` (1, 21, 31…), `few` (2–4, 22–24…), `many` (the rest,
/// including 11–14).
pub fn plural_uk<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    match (n % 10, n % 100) {
        (1, r) if r != 11 => one,
        (2..=4, r) if !(12..=14).contains(&r) => few,
        _ => many,
    }
}
