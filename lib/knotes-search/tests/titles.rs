use knotes_core::TitleSearch;
use knotes_search::TitleIndex;

fn index(entries: &[(&str, &[&str])]) -> TitleIndex {
    TitleIndex::new(entries.iter().map(|(p, a)| (p.to_string(), a.iter().map(|s| s.to_string()).collect())))
}

fn paths(index: &TitleIndex, query: &str) -> Vec<String> {
    index.search_titles(query, 20).into_iter().map(|h| h.path).collect()
}

#[test]
fn fuzzy_and_case_insensitive() {
    let i = index(&[("Notes/Radar basics.md", &[]), ("Notes/Cooking.md", &[])]);
    assert_eq!(paths(&i, "rdr"), ["Notes/Radar basics.md"]);
    assert_eq!(paths(&i, "RADAR"), ["Notes/Radar basics.md"]);
}

#[test]
fn title_match_beats_folder_match() {
    let i = index(&[("Radar/notes.md", &[]), ("Projects/Radar.md", &[])]);
    assert_eq!(paths(&i, "radar"), ["Projects/Radar.md", "Radar/notes.md"]);
}

#[test]
fn query_can_span_folder_and_title() {
    let i = index(&[("Projects/Radar.md", &[]), ("Radar/notes.md", &[])]);
    assert_eq!(paths(&i, "proj rad"), ["Projects/Radar.md"]);
}

#[test]
fn ukrainian_titles() {
    let i = index(&[("Проєкти/Радар ЧМ.md", &[]), ("Проєкти/Кулінарія.md", &[])]);
    assert_eq!(paths(&i, "радар"), ["Проєкти/Радар ЧМ.md"]);
    assert_eq!(paths(&i, "рдр чм"), ["Проєкти/Радар ЧМ.md"]);
}

#[test]
fn aliases_match_and_paths_are_not_repeated() {
    let i = index(&[("People/Ivan Petrenko.md", &["Ivan", "Іван"]), ("People/Olena.md", &[])]);
    let hits = i.search_titles("іван", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].path.as_str(), hits[0].alias.as_deref()), ("People/Ivan Petrenko.md", Some("Іван")));
    assert_eq!(paths(&i, "ivan"), ["People/Ivan Petrenko.md"]);
}

#[test]
fn fzf_syntax() {
    let i = index(&[("Radar draft.md", &[]), ("Radar final.md", &[]), ("Old radar.md", &[])]);
    assert_eq!(paths(&i, "!draft ^radar").len(), 1);
    assert_eq!(paths(&i, "!draft ^radar"), ["Radar final.md"]);
    assert_eq!(paths(&i, "'old"), ["Old radar.md"]);
}

#[test]
fn extension_is_not_matched_but_other_files_keep_theirs() {
    let i = index(&[("a.md", &[]), ("b.md", &[]), ("att/Paper.pdf", &[])]);
    assert!(paths(&i, "md").is_empty());
    assert_eq!(paths(&i, "pdf"), ["att/Paper.pdf"]);
}

#[test]
fn empty_query_and_limit() {
    let entries: Vec<(String, Vec<String>)> = (0..30).map(|i| (format!("note {i}.md"), vec![])).collect();
    let i = TitleIndex::new(entries);
    assert!(i.search_titles("", 10).is_empty());
    assert_eq!(i.search_titles("note", 10).len(), 10);
}
