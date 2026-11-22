use knotes_app::{FolderRules, Moves, folder_star, moved_stars, starred_folder, toggled_star, with_stars};

fn stars(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn stars_are_kept_in_the_root_rules_file_beside_its_other_keys() {
    let text = with_stars(r#"{"trash": {"days": 14}}"#, &stars(&["Projects/", "Projects/Plan.md"])).unwrap();
    let rules = FolderRules::parse(&text).unwrap();
    assert_eq!(rules.stars(), stars(&["Projects/", "Projects/Plan.md"]));
    assert_eq!(rules.trash_days(), 14, "the other keys stay");
    let none = with_stars(&text, &[]).unwrap();
    assert!(!none.contains("stars") && FolderRules::parse(&none).unwrap().trash_days() == 14, "{none}");
    assert_eq!(with_stars("", &[]).unwrap(), "", "no file made for no stars");
    assert_eq!(FolderRules::parse(&with_stars("", &stars(&["a.md"])).unwrap()).unwrap().stars(), stars(&["a.md"]), "a file not there yet");
    assert!(with_stars("{broken", &stars(&["a.md"])).is_err(), "a file that does not parse is left");
    assert_eq!(FolderRules::parse(r#"{"stars": ["a.md", 7, "a.md", "/", ""]}"#).unwrap().stars(), stars(&["a.md"]), "each once, only paths");
}

#[test]
fn a_star_is_put_last_or_taken_out() {
    let list = toggled_star(stars(&["a.md"]), &folder_star("Projects"));
    assert_eq!(list, stars(&["a.md", "Projects/"]));
    assert_eq!(toggled_star(list, "a.md"), stars(&["Projects/"]));
    assert_eq!((starred_folder("Projects/"), starred_folder("Projects/Plan.md")), (Some("Projects"), None));
}

#[test]
fn a_star_follows_its_file_and_its_folder_when_they_move() {
    let list = stars(&["Old/", "Old/Sub/", "Old/a.md", "Older/b.md", "c.md"]);
    let moves = Moves::from([("c.md".to_string(), "Archive/c.md".to_string())]);
    assert_eq!(moved_stars(&list, &moves, None), stars(&["Old/", "Old/Sub/", "Old/a.md", "Older/b.md", "Archive/c.md"]));
    let folder = Moves::from([("Old/a.md".to_string(), "New/a.md".to_string())]);
    assert_eq!(moved_stars(&list, &folder, Some(("Old", "New"))), stars(&["New/", "New/Sub/", "New/a.md", "Older/b.md", "c.md"]), "not a folder whose name only starts so");
}
