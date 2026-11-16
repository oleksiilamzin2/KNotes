use std::collections::HashSet;

use knotes_app::{SpaceFolders, SpaceTags, count_tags, folder_tree};
use knotes_spaces::Spaces;

#[test]
fn every_space_s_tags_counted_in_one_pass_are_those_counted_space_by_space() {
    let paths = ["Work/a.md", "Work/Radar/b.md", "Home/c.md", "Home/e.md", "Shared/d.md", "top.md"];
    let tree = folder_tree(paths);
    let rules = [
        ("", r#"{"spaces": [{"id": "work"}, {"id": "home"}]}"#),
        ("Work", r#"{"space": "work"}"#),
        ("Home", r#"{"space": "home"}"#),
        ("Shared", r#"{"space": "*"}"#),
    ];
    let spaces = Spaces::from_texts(&rules.map(|(folder, text)| (folder.to_owned(), text.to_owned())));
    // A note's tags together, as the index gives them.
    let tagged: Vec<(String, String)> = [
        ("area/job", "Work/a.md"),
        ("radar", "Work/a.md"),
        ("radar", "Work/Radar/b.md"),
        ("area/home", "Home/c.md"),
        ("cooking", "Home/c.md"),
        ("contacts", "Shared/d.md"),
        ("cooking", "top.md"),
        ("radar", "top.md"),
    ]
    .map(|(tag, path)| (tag.to_owned(), path.to_owned()))
    .to_vec();
    let untagged: HashSet<String> = ["Home/e.md", "Shared/f.md", "top2.md"].map(str::to_owned).into();
    let folders = |id: &str| SpaceFolders::new(&tree, &spaces, id);
    let all = count_tags(tagged.iter().map(|(tag, path)| (tag.as_str(), path.as_str())));
    let counted = SpaceTags::count_all(vec![("work".to_owned(), folders("work")), ("home".to_owned(), folders("home"))], &tagged, &untagged, &all);
    let one_by_one = [SpaceTags::count("work", folders("work"), &tagged, &untagged), SpaceTags::count("home", folders("home"), &tagged, &untagged)];
    assert_eq!(counted, one_by_one);
    assert_eq!(counted[1].untagged, 2, "its own and the one in every space");
    assert!(counted[0].tags.iter().any(|t| t.tag == "contacts"), "a folder in every space counts in each");
}
