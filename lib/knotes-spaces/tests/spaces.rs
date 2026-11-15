use knotes_spaces::{SpaceColor, Spaces};

/// The root's rules with two spaces, and folders' rules.
fn vault(folders: &[(&str, &str)]) -> Spaces {
    let mut rules = vec![(String::new(), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}, {"id": "home", "name": "Дім", "color": "green"}], "space": "work"}"#.to_string())];
    rules.extend(folders.iter().map(|(folder, text)| (folder.to_string(), text.to_string())));
    Spaces::from_texts(&rules)
}

fn space_of<'a>(spaces: &'a Spaces, path: &str) -> Option<&'a str> {
    spaces.of(path).map(|s| s.id.as_str())
}

#[test]
fn the_root_lists_the_spaces_in_order_with_their_names_and_colours() {
    let spaces = vault(&[]);
    let listed: Vec<(&str, &str, SpaceColor)> = spaces.list().iter().map(|s| (s.id.as_str(), s.name.as_str(), s.color)).collect();
    assert_eq!(listed, [("work", "Робота", SpaceColor::Blue), ("home", "Дім", SpaceColor::Green)]);
    assert_eq!(spaces.get("home").map(|s| s.name.as_str()), Some("Дім"));
    assert!(Spaces::from_texts(&[]).is_empty());
}

#[test]
fn a_document_is_in_its_folder_s_space_taken_from_the_nearest_folder_that_names_one() {
    let spaces = vault(&[("Робота", r#"{"space": "work"}"#), ("Робота/Архів", r#"{"space": null}"#), ("Дім", r#"{"space": "home", "list": {"sort": "title"}}"#), ("Дім/Кухня/Рецепти", r#"{"list": {"sort": "modified"}}"#)]);
    assert_eq!(space_of(&spaces, "Робота/План.md"), Some("work"));
    assert_eq!(space_of(&spaces, "Робота/Проєкти/Радар.md"), Some("work"), "subfolders take it");
    assert_eq!(space_of(&spaces, "Робота/Архів/2024.md"), None, "null takes a subfolder out");
    assert_eq!(space_of(&spaces, "Робота/Архів/Старе/x.md"), None);
    assert_eq!(space_of(&spaces, "Дім/Кухня/Рецепти/Борщ.md"), Some("home"), "a folder's other rules do not stop it");
    assert_eq!(space_of(&spaces, "Нотатка.md"), None, "the root is in no space, whatever its file says");
    assert_eq!(space_of(&spaces, "Інше/x.md"), None);
    assert_eq!(spaces.of_folder("Робота/Проєкти").map(|s| s.id.as_str()), Some("work"));
    assert_eq!(spaces.of_folder(""), None);
}

#[test]
fn an_unknown_space_counts_as_none_and_stops_what_the_parent_gives() {
    let spaces = vault(&[("Робота", r#"{"space": "work"}"#), ("Робота/Чуже", r#"{"space": "gone"}"#), ("Робота/Число", r#"{"space": 3}"#)]);
    assert_eq!(space_of(&spaces, "Робота/Чуже/x.md"), None);
    assert_eq!(space_of(&spaces, "Робота/Число/x.md"), Some("work"), "a value of the wrong type is ignored");
}

#[test]
fn paths_are_compared_composed() {
    // "й" written decomposed (и + ˘), as macOS writes some names, and composed.
    let (decomposed, composed) = ("Мои\u{0306}", "Мой");
    let spaces = vault(&[(decomposed, r#"{"space": "home"}"#)]);
    assert_eq!(space_of(&spaces, &format!("{composed}/x.md")), Some("home"));
    let spaces = vault(&[(composed, r#"{"space": "home"}"#)]);
    assert_eq!(space_of(&spaces, &format!("{decomposed}/x.md")), Some("home"));
}

#[test]
fn broken_rules_and_bad_spaces_are_skipped() {
    let rules = [
        (String::new(), r#"{"spaces": [{"id": "work", "color": "chartreuse"}, {"name": "без id"}, {"id": "work", "name": "Двійник"}, "text", {"id": ""}]}"#.to_string()),
        ("Робота".to_string(), "{not json".to_string()),
        ("Інше".to_string(), r#"{"space": "work"}"#.to_string()),
    ];
    let spaces = Spaces::from_texts(&rules);
    let listed: Vec<(&str, &str, SpaceColor)> = spaces.list().iter().map(|s| (s.id.as_str(), s.name.as_str(), s.color)).collect();
    assert_eq!(listed, [("work", "work", SpaceColor::Gray)], "the name defaults to the id, an unknown colour is gray, the first of an id wins");
    assert_eq!(space_of(&spaces, "Робота/x.md"), None);
    assert_eq!(space_of(&spaces, "Інше/x.md"), Some("work"));
}

#[test]
fn colours_go_by_their_names() {
    for colour in SpaceColor::ALL {
        assert_eq!(SpaceColor::from_name(colour.name()), colour);
    }
    assert_eq!(SpaceColor::ALL.len(), 9);
    assert_eq!(SpaceColor::from_name("BLUE"), SpaceColor::Gray, "names are lower case");
}
