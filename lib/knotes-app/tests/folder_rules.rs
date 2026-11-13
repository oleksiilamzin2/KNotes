use std::collections::HashMap;

use knotes_app::{FolderRules, ListSettings, SortOrder, resolve_list_settings};

fn rules(entries: &[(&str, &str)]) -> HashMap<String, FolderRules> {
    entries.iter().map(|(folder, json)| (folder.to_string(), FolderRules::parse(json).unwrap())).collect()
}

#[test]
fn settings_are_inherited_field_by_field_from_ancestors() {
    let r = rules(&[("", r#"{"list": {"sort": "title"}}"#), ("Journal", r#"{"list": {"groupByDate": false}}"#)]);
    let journal_2026 = resolve_list_settings("Journal/2026", &r);
    assert_eq!(journal_2026, ListSettings { sort: SortOrder::Title, group_by_date: false, include_subfolders: false });
    let other = resolve_list_settings("Other", &r);
    assert_eq!(other, ListSettings { sort: SortOrder::Title, ..ListSettings::default() });
    assert_eq!(resolve_list_settings("", &HashMap::new()), ListSettings::default());
}

#[test]
fn own_rules_win_over_inherited_ones() {
    let r = rules(&[("", r#"{"list": {"sort": "title"}}"#), ("Journal/2026", r#"{"list": {"sort": "modified", "includeSubfolders": false}}"#)]);
    let s = resolve_list_settings("Journal/2026", &r);
    assert_eq!((s.sort, s.include_subfolders), (SortOrder::Modified, false));
}

#[test]
fn writing_list_settings_keeps_unknown_keys() {
    let mut r = FolderRules::parse(r##"{"graph": {"color": "#ff0000"}, "list": {"note": "kept"}}"##).unwrap();
    r.set_list(&ListSettings { sort: SortOrder::Title, group_by_date: false, include_subfolders: true });
    let back: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(back["graph"]["color"], "#ff0000");
    assert_eq!(back["list"]["note"], "kept");
    assert_eq!(back["list"]["sort"], "title");
    assert_eq!(back["list"]["groupByDate"], false);
    assert!(r.to_json().ends_with('\n'));
}

#[test]
fn broken_or_odd_files() {
    assert!(FolderRules::parse("{not json").is_err());
    assert!(FolderRules::parse("[1, 2]").is_err());
    let odd = rules(&[("", r#"{"list": {"sort": "weird", "groupByDate": "yes"}}"#)]);
    assert_eq!(resolve_list_settings("", &odd), ListSettings::default());
}

#[test]
fn a_menu_choice_writes_only_its_own_field() {
    use knotes_app::ListOption;
    let mut r = FolderRules::parse(r#"{"list": {"note": "kept"}}"#).unwrap();
    r.set_option(ListOption::SortBy(SortOrder::Title));
    let back: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(back["list"]["sort"], "title");
    assert!(back["list"].get("groupByDate").is_none() && back["list"].get("includeSubfolders").is_none());
    assert_eq!(back["list"]["note"], "kept");
    r.set_option(ListOption::GroupByDate(false));
    r.set_option(ListOption::IncludeSubfolders(false));
    let back: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!((back["list"]["groupByDate"].clone(), back["list"]["includeSubfolders"].clone()), (false.into(), false.into()));
}

#[test]
fn options_change_settings() {
    use knotes_app::ListOption;
    let s = ListSettings::default().with(ListOption::SortBy(SortOrder::Title)).with(ListOption::GroupByDate(false));
    assert_eq!(s, ListSettings { sort: SortOrder::Title, group_by_date: false, include_subfolders: false });
}

#[test]
fn new_notes_go_to_the_root_rules_folder_or_the_default() {
    use knotes_app::new_notes_folder;
    assert_eq!(new_notes_folder(&rules(&[]), "Вхідні"), "Вхідні");
    assert_eq!(new_notes_folder(&rules(&[("", r#"{"newNotes": {"folder": "Inbox/New"}}"#)]), "Вхідні"), "Inbox/New");
    assert_eq!(new_notes_folder(&rules(&[("", r#"{"newNotes": {"folder": "/Inbox/"}}"#)]), "Вхідні"), "Inbox", "slashes trimmed");
    assert_eq!(new_notes_folder(&rules(&[("", r#"{"newNotes": {"folder": ""}}"#)]), "Вхідні"), "", "empty means the vault root");
    assert_eq!(new_notes_folder(&rules(&[("A", r#"{"newNotes": {"folder": "X"}}"#)]), "Вхідні"), "Вхідні", "only the root decides");
}
