use std::collections::HashSet;

use knotes_app::{Clock, DateGroup, ListFilter, ListItem, ListSettings, SortOrder, build_note_list};
use knotes_core::NoteRow;

const H: i64 = 3_600_000_000_000;
const D: i64 = 24 * H;
/// 2026-11-12 10:00 in UTC+2.
const NOW: i64 = 1_794_470_400_000_000_000;
const CLOCK: Clock = Clock { now_ns: NOW, utc_offset_s: 7200 };

fn row(path: &str, mtime_ns: i64) -> NoteRow {
    NoteRow { path: path.into(), mtime_ns, snippet: String::new() }
}

fn notes(rows: &[NoteRow], items: &[ListItem]) -> Vec<String> {
    items.iter().filter_map(|i| if let ListItem::Note(n) = i { Some(rows[*n as usize].path.clone()) } else { None }).collect()
}

fn flat(sort: SortOrder) -> ListSettings {
    ListSettings { sort, group_by_date: false, include_subfolders: true }
}

#[test]
fn folder_filter_with_and_without_subfolders() {
    let rows = vec![row("P/a.md", 1), row("P/Sub/b.md", 2), row("Q/c.md", 3), row("top.md", 4)];
    let all = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Title), &CLOCK);
    assert_eq!(notes(&rows, &all).len(), 4);
    let deep = build_note_list(&rows, &ListFilter::Folder("P"), &flat(SortOrder::Title), &CLOCK);
    assert_eq!(notes(&rows, &deep), ["P/a.md", "P/Sub/b.md"]);
    let direct = ListSettings { include_subfolders: false, ..flat(SortOrder::Title) };
    let only = build_note_list(&rows, &ListFilter::Folder("P"), &direct, &CLOCK);
    assert_eq!(notes(&rows, &only), ["P/a.md"]);
    let tagged: HashSet<String> = ["Q/c.md".to_owned()].into();
    let by_tag = build_note_list(&rows, &ListFilter::Paths(&tagged), &flat(SortOrder::Title), &CLOCK);
    assert_eq!(notes(&rows, &by_tag), ["Q/c.md"]);
}

#[test]
fn sorting() {
    let rows = vec![row("note 10.md", 5), row("Note 2.md", 5), row("alpha.md", 1), row("zeta.md", 9)];
    let by_time = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Modified), &CLOCK);
    assert_eq!(notes(&rows, &by_time), ["zeta.md", "Note 2.md", "note 10.md", "alpha.md"]);
    let by_title = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Title), &CLOCK);
    assert_eq!(notes(&rows, &by_title), ["alpha.md", "Note 2.md", "note 10.md", "zeta.md"]);
}

#[test]
fn date_groups_like_apple_notes() {
    let midnight_local = NOW - 10 * H; // 2026-11-12 00:00 local
    let rows = vec![
        row("today.md", NOW - H),
        row("just after midnight.md", midnight_local + H / 2),
        row("yesterday.md", midnight_local - H),
        row("week.md", NOW - 3 * D),
        row("month.md", NOW - 20 * D),
        row("march.md", NOW - 252 * D),
        row("last year.md", NOW - 330 * D),
    ];
    let settings = ListSettings { sort: SortOrder::Modified, group_by_date: true, include_subfolders: true };
    let items = build_note_list(&rows, &ListFilter::All, &settings, &CLOCK);
    let shape: Vec<String> = items
        .iter()
        .map(|i| match i {
            ListItem::Header(g) => format!("{g:?}"),
            ListItem::Note(n) | ListItem::Found(n, _) => rows[*n as usize].path.clone(),
        })
        .collect();
    assert_eq!(
        shape,
        [
            format!("{:?}", DateGroup::Today),
            "today.md".into(),
            "just after midnight.md".into(),
            format!("{:?}", DateGroup::Yesterday),
            "yesterday.md".into(),
            format!("{:?}", DateGroup::Previous7Days),
            "week.md".into(),
            format!("{:?}", DateGroup::Previous30Days),
            "month.md".into(),
            format!("{:?}", DateGroup::Month { year: 2026, month: 3 }),
            "march.md".into(),
            format!("{:?}", DateGroup::Year(2025)),
            "last year.md".into(),
        ]
    );
}

#[test]
fn no_headers_when_sorted_by_title() {
    let rows = vec![row("a.md", NOW), row("b.md", NOW - 40 * D)];
    let settings = ListSettings { sort: SortOrder::Title, group_by_date: true, include_subfolders: true };
    let items = build_note_list(&rows, &ListFilter::All, &settings, &CLOCK);
    assert!(items.iter().all(|i| matches!(i, ListItem::Note(_))));
}
