use knotes_app::{folder_space_mark, space_mark};
use knotes_spaces::Spaces;

fn vault() -> Spaces {
    Spaces::from_texts(&[
        (String::new(), r#"{"spaces": [{"id": "work", "name": "Робота"}, {"id": "home", "name": "Дім"}]}"#.into()),
        ("Work".into(), r#"{"space": "work"}"#.into()),
        ("Home".into(), r#"{"space": "home"}"#.into()),
    ])
}

fn mark(spaces: &Spaces, current: Option<&str>, path: &str) -> Option<String> {
    space_mark(spaces, current, path).map(|s| s.id.clone())
}

#[test]
fn a_file_shows_its_space_s_mark_in_the_whole_vault_and_only_another_space_s_in_one() {
    let spaces = vault();
    assert_eq!(mark(&spaces, None, "Work/Radar/a.md"), Some("work".into()), "the whole vault: every file in a space");
    assert_eq!(mark(&spaces, None, "top.md"), None);
    assert_eq!(mark(&spaces, Some("work"), "Work/a.md"), None, "its own space's files go unmarked");
    assert_eq!(mark(&spaces, Some("work"), "Home/b.md"), Some("home".into()), "another space's");
    assert_eq!(mark(&spaces, Some("work"), "top.md"), None, "in no space");
}

#[test]
fn a_folder_shows_a_mark_where_its_space_is_set_unless_it_is_the_window_s() {
    let spaces = vault();
    assert_eq!(folder_space_mark(&spaces, None, "Work").map(|s| s.id.as_str()), Some("work"));
    assert_eq!(folder_space_mark(&spaces, None, "Work/Radar"), None, "a subfolder takes it: no mark on each");
    assert_eq!(folder_space_mark(&spaces, Some("work"), "Work"), None, "in a space its own folders go unmarked");
    assert_eq!(folder_space_mark(&spaces, Some("work"), "Home").map(|s| s.id.as_str()), Some("home"), "another space's, under «Решта бази», marked");
}
