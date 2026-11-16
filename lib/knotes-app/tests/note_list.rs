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
    let all = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Title), &CLOCK, None);
    assert_eq!(notes(&rows, &all).len(), 4);
    let deep = build_note_list(&rows, &ListFilter::Folder("P"), &flat(SortOrder::Title), &CLOCK, None);
    assert_eq!(notes(&rows, &deep), ["P/a.md", "P/Sub/b.md"]);
    let direct = ListSettings { include_subfolders: false, ..flat(SortOrder::Title) };
    let only = build_note_list(&rows, &ListFilter::Folder("P"), &direct, &CLOCK, None);
    assert_eq!(notes(&rows, &only), ["P/a.md"]);
    let tagged: HashSet<String> = ["Q/c.md".to_owned()].into();
    let by_tag = build_note_list(&rows, &ListFilter::Paths(&tagged), &flat(SortOrder::Title), &CLOCK, None);
    assert_eq!(notes(&rows, &by_tag), ["Q/c.md"]);
}

#[test]
fn sorting() {
    let rows = vec![row("note 10.md", 5), row("Note 2.md", 5), row("alpha.md", 1), row("zeta.md", 9)];
    let by_time = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Modified), &CLOCK, None);
    assert_eq!(notes(&rows, &by_time), ["zeta.md", "Note 2.md", "note 10.md", "alpha.md"]);
    let by_title = build_note_list(&rows, &ListFilter::All, &flat(SortOrder::Title), &CLOCK, None);
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
    let items = build_note_list(&rows, &ListFilter::All, &settings, &CLOCK, None);
    let shape: Vec<String> = items
        .iter()
        .map(|i| match i {
            ListItem::Header(g) => format!("{g:?}"),
            ListItem::Rest => "[rest]".into(),
            ListItem::Deleted(_) => "[deleted]".into(),
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
    let items = build_note_list(&rows, &ListFilter::All, &settings, &CLOCK, None);
    assert!(items.iter().all(|i| matches!(i, ListItem::Note(_))));
}

#[test]
fn a_folder_s_list_shows_its_notes_and_its_search_covers_its_pdfs_too() {
    let rows = vec![row("P/a.md", 1), row("P/doc.pdf", 2), row("Q/c.md", 3)];
    let list = build_note_list(&rows, &ListFilter::Folder("P"), &flat(SortOrder::Title), &CLOCK, None);
    assert_eq!(notes(&rows, &list), ["P/a.md"], "no PDF among the notes");
    assert_eq!(knotes_app::list_scope(&rows, &ListFilter::Folder("P"), &flat(SortOrder::Title), None), ["P/a.md", "P/doc.pdf"]);
}

/// A list's rows: each note's path, the «Решта бази» header as `[rest]`, date headers left out.
fn parts(rows: &[NoteRow], items: &[ListItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            ListItem::Note(n) | ListItem::Found(n, _) => Some(rows[*n as usize].path.clone()),
            ListItem::Rest => Some("[rest]".into()),
            ListItem::Deleted(_) => None,
            ListItem::Header(_) => None,
        })
        .collect()
}

#[test]
fn a_list_in_a_space_gives_its_notes_then_the_rest_of_the_vault_s_under_a_header() {
    use knotes_app::{SpaceFolders, SpaceList, folder_tree, list_scope};
    use knotes_spaces::Spaces;
    let rows = vec![row("Work/a.md", 1), row("Work/Radar/b.md", 2), row("Work/Archive/c.md", 3), row("Home/d.md", 4), row("top.md", 5), row("Work/e.pdf", 6)];
    let tree = folder_tree(rows.iter().map(|r| r.path.as_str()));
    let spaces = Spaces::from_texts(&[
        (String::new(), r#"{"spaces": [{"id": "work"}, {"id": "home"}]}"#.into()),
        ("Work".into(), r#"{"space": "work"}"#.into()),
        ("Work/Archive".into(), r#"{"space": null}"#.into()),
        ("Home".into(), r#"{"space": "home"}"#.into()),
    ]);
    let work = SpaceFolders::new(&tree, &spaces, "work");
    assert!(work.holds("Work/Radar/b.md") && !work.holds("Work/Archive/c.md") && !work.holds("top.md"));
    assert!(work.holds_folder("Work/Radar") && !work.holds_folder("Home") && !work.holds_folder(""));
    let space = Some(SpaceList::Space(&work));
    let title = flat(SortOrder::Title);
    let all = build_note_list(&rows, &ListFilter::All, &title, &CLOCK, space);
    assert_eq!(parts(&rows, &all), ["Work/a.md", "Work/Radar/b.md", "[rest]", "Work/Archive/c.md", "Home/d.md", "top.md"], "the space's notes, then the rest of the vault's");
    let folder = build_note_list(&rows, &ListFilter::Folder("Work"), &title, &CLOCK, space);
    assert_eq!(parts(&rows, &folder), ["Work/a.md", "Work/Radar/b.md", "[rest]", "Work/Archive/c.md"], "the folder's taken-out subfolder below");
    let tagged: HashSet<String> = ["Home/d.md".to_owned(), "Work/a.md".to_owned()].into();
    let by_tag = build_note_list(&rows, &ListFilter::Paths(&tagged), &title, &CLOCK, space);
    assert_eq!(parts(&rows, &by_tag), ["Work/a.md", "[rest]", "Home/d.md"]);
    let radar = build_note_list(&rows, &ListFilter::Folder("Work/Radar"), &title, &CLOCK, space);
    assert_eq!(parts(&rows, &radar), ["Work/Radar/b.md"], "nothing outside: no header");
    let rest = build_note_list(&rows, &ListFilter::All, &title, &CLOCK, Some(SpaceList::Rest(&work)));
    assert_eq!(parts(&rows, &rest), ["Work/Archive/c.md", "Home/d.md", "top.md"], "the rest alone, no header");
    let grouped = ListSettings { sort: SortOrder::Modified, group_by_date: true, include_subfolders: true };
    let dated = build_note_list(&rows, &ListFilter::All, &grouped, &CLOCK, space);
    let after = dated.iter().position(|i| *i == ListItem::Rest).unwrap();
    assert!(dated[..after].iter().any(|i| matches!(i, ListItem::Header(_))) && !dated[after..].iter().any(|i| matches!(i, ListItem::Header(_))), "date headers over the space's notes only: {dated:?}");
    assert_eq!(list_scope(&rows, &ListFilter::All, &title, space).len(), 6, "a search covers both parts");
    assert_eq!(list_scope(&rows, &ListFilter::All, &title, Some(SpaceList::Rest(&work))), ["Work/Archive/c.md", "Home/d.md", "top.md"]);
    let whole = build_note_list(&rows, &ListFilter::All, &title, &CLOCK, None);
    assert_eq!(notes(&rows, &whole).len(), 5, "no space: the whole vault");
}
