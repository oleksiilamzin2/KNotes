use std::collections::HashSet;

use knotes_core::NoteRow;

use crate::natural_order::{natural_cmp, natural_key};

/// Current time and the local UTC offset (from the system), for date groups.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub now_ns: i64,
    pub utc_offset_s: i32,
}

/// Date sections of a list sorted by modification time, as in Apple Notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateGroup {
    Today,
    Yesterday,
    Previous7Days,
    Previous30Days,
    /// A month of the current year.
    Month { year: i32, month: u8 },
    /// An earlier year.
    Year(i32),
}

pub enum ListFilter<'a> {
    All,
    /// Notes in a folder (`""` is the vault root).
    Folder(&'a str),
    /// Notes with these paths, e.g. the notes of a tag.
    Paths(&'a HashSet<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Newest first.
    Modified,
    /// Natural order of titles.
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListSettings {
    pub sort: SortOrder,
    /// Date headers; only with `SortOrder::Modified`.
    pub group_by_date: bool,
    /// A folder also lists notes of its subfolders.
    pub include_subfolders: bool,
}

impl Default for ListSettings {
    fn default() -> Self {
        Self { sort: SortOrder::Modified, group_by_date: true, include_subfolders: true }
    }
}

/// A row of the note list: a date header or an index into the rows given to `build_note_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListItem {
    Header(DateGroup),
    Note(u32),
}

/// Title shown for a note: its file name without `.md`.
pub fn title_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name)
}

pub fn build_note_list(rows: &[NoteRow], filter: &ListFilter, settings: &ListSettings, clock: &Clock) -> Vec<ListItem> {
    let keep = |row: &NoteRow| match filter {
        ListFilter::All => true,
        ListFilter::Paths(paths) => paths.contains(&row.path),
        ListFilter::Folder(folder) => {
            let parent = row.path.rsplit_once('/').map_or("", |(dir, _)| dir);
            parent == *folder
                || (settings.include_subfolders
                    && (folder.is_empty() || parent.strip_prefix(*folder).is_some_and(|rest| rest.starts_with('/'))))
        }
    };
    let mut picked: Vec<u32> = (0..rows.len() as u32).filter(|&i| keep(&rows[i as usize])).collect();
    let title = |i: u32| title_of(&rows[i as usize].path);
    match settings.sort {
        SortOrder::Modified => picked.sort_by(|&a, &b| {
            rows[b as usize].mtime_ns.cmp(&rows[a as usize].mtime_ns).then_with(|| natural_cmp(title(a), title(b)))
        }),
        SortOrder::Title => picked.sort_by_cached_key(|&i| (natural_key(title(i)), rows[i as usize].path.clone())),
    }
    if !(settings.group_by_date && settings.sort == SortOrder::Modified) {
        return picked.into_iter().map(ListItem::Note).collect();
    }
    let mut items = Vec::with_capacity(picked.len() + 16);
    let mut current = None;
    for i in picked {
        let group = date_group(rows[i as usize].mtime_ns, clock);
        if current != Some(group) {
            items.push(ListItem::Header(group));
            current = Some(group);
        }
        items.push(ListItem::Note(i));
    }
    items
}

/// Days since 1970-01-01 in local time.
pub(crate) fn local_day(ns: i64, clock: &Clock) -> i64 {
    (ns.div_euclid(1_000_000_000) + i64::from(clock.utc_offset_s)).div_euclid(86_400)
}

fn date_group(mtime_ns: i64, clock: &Clock) -> DateGroup {
    let (today, day) = (local_day(clock.now_ns, clock), local_day(mtime_ns, clock));
    match today - day {
        i64::MIN..=0 => DateGroup::Today,
        1 => DateGroup::Yesterday,
        2..=7 => DateGroup::Previous7Days,
        8..=30 => DateGroup::Previous30Days,
        _ => {
            let (year, month) = civil_from_days(day);
            if year == civil_from_days(today).0 { DateGroup::Month { year, month } } else { DateGroup::Year(year) }
        }
    }
}

/// Days since 1970-01-01 of a calendar date (Howard Hinnant's days-from-civil).
pub fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Year and month of a day count since 1970-01-01 (Howard Hinnant's civil-from-days).
fn civil_from_days(days: i64) -> (i32, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as i32, month as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 12), 20_769);
        for days in [-1000, 0, 59, 60, 20_769, 40_000] {
            let (y, m) = civil_from_days(days);
            assert!(days_from_civil(y, m, 1) <= days && days < days_from_civil(y, m, 1) + 31);
        }
    }
}
