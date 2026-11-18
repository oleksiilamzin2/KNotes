use knotes_spaces::{FolderSpace, Space, SpaceColor, Spaces, new_space, new_space_id, set_folder_space, set_spaces};
use serde_json::{Value, json};

fn space(id: &str, name: &str, color: SpaceColor) -> Space {
    Space { id: id.into(), name: name.into(), color }
}

#[test]
fn a_folder_s_space_is_written_into_its_rules_keeping_the_rest() {
    let written = set_folder_space(r#"{"list": {"sort": "title"}}"#, FolderSpace::In("work")).unwrap();
    assert!(written.ends_with('\n'));
    let value: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value, json!({"list": {"sort": "title"}, "space": "work"}));
    let none: Value = serde_json::from_str(&set_folder_space(&written, FolderSpace::None).unwrap()).unwrap();
    assert_eq!(none["space"], Value::Null, "null takes a subfolder out");
    let inherited: Value = serde_json::from_str(&set_folder_space(&written, FolderSpace::Inherit).unwrap()).unwrap();
    assert_eq!(inherited, json!({"list": {"sort": "title"}}), "the key goes: the parent's space again");
    let fresh: Value = serde_json::from_str(&set_folder_space("", FolderSpace::In("home")).unwrap()).unwrap();
    assert_eq!(fresh, json!({"space": "home"}), "no file yet");
    assert!(set_folder_space("[1]", FolderSpace::In("home")).is_err(), "not a JSON object: left alone");
}

#[test]
fn the_spaces_are_written_into_the_root_s_rules_in_order_keeping_the_rest() {
    let spaces = [space("work", "Робота", SpaceColor::Blue), space("home", "Дім", SpaceColor::Green)];
    let written = set_spaces(r#"{"newNotes": {"folder": "Вхідні"}, "spaces": [{"id": "old"}]}"#, &spaces).unwrap();
    assert_eq!(Spaces::from_texts(&[(String::new(), written.clone())]).list(), spaces);
    let value: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value["newNotes"], json!({"folder": "Вхідні"}));
    assert_eq!(value["spaces"][0], json!({"id": "work", "name": "Робота", "color": "blue"}));
    let none: Value = serde_json::from_str(&set_spaces(&written, &[]).unwrap()).unwrap();
    assert_eq!(none.get("spaces"), None, "no spaces, no key");
}

#[test]
fn a_new_space_gets_an_id_from_its_name_that_no_other_has() {
    assert_eq!(new_space_id(&[], "Work stuff!"), "work-stuff");
    assert_eq!(new_space_id(&[], "Робота й дім"), "robota-i-dim", "Ukrainian transliterated");
    assert_eq!(new_space_id(&[], "Щоденник'ї"), "shchodennyki");
    assert_eq!(new_space_id(&[space("robota", "Робота", SpaceColor::Blue)], "Робота"), "robota-2");
    assert_eq!(new_space_id(&[space("robota", "", SpaceColor::Blue), space("robota-2", "", SpaceColor::Blue)], "Робота"), "robota-3");
    assert_eq!(new_space_id(&[], "🙂"), "space", "nothing to spell it with");
}

#[test]
fn a_folder_in_every_space_is_written_as_a_star() {
    let written: Value = serde_json::from_str(&set_folder_space("{}", FolderSpace::Shared).unwrap()).unwrap();
    assert_eq!(written, json!({"space": "*"}));
}

#[test]
fn a_new_space_takes_the_first_colour_no_space_has() {
    let made = new_space(&[], "Робота");
    assert_eq!((made.id.as_str(), made.name.as_str(), made.color), ("robota", "Робота", SpaceColor::Blue));
    let two = [space("a", "A", SpaceColor::Blue), space("b", "B", SpaceColor::Pink)];
    assert_eq!(new_space(&two, "C").color, SpaceColor::Purple, "the first free, not the one after the last");
    assert_eq!(new_space(&two, "A").id, "a-2");
    // All nine taken: round the palette again. One's own colour takes none of them.
    let mut all: Vec<Space> = SpaceColor::ALL.iter().enumerate().map(|(i, color)| space(&format!("s{i}"), "", *color)).collect();
    assert_eq!(new_space(&all, "X").color, SpaceColor::Blue);
    all.push(space("own", "", SpaceColor::Custom(1, 2, 3)));
    assert_eq!(new_space(&all, "X").color, SpaceColor::Purple);
    assert_eq!(new_space(&[space("own", "", SpaceColor::Custom(1, 2, 3))], "X").color, SpaceColor::Blue);
    assert_eq!(SpaceColor::from_srgb(1.0, 0.5, 0.0), SpaceColor::Custom(255, 128, 0));
    assert_eq!(SpaceColor::from_srgb(1.4, -0.2, 0.999), SpaceColor::Custom(255, 0, 255), "past an end is the end");
}

#[test]
fn a_folder_says_its_space_only_when_its_parent_s_is_another() {
    let rules = [
        (String::new(), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}, {"id": "home", "name": "Дім", "color": "green"}]}"#.to_owned()),
        ("Work".to_owned(), r#"{"space": "work"}"#.to_owned()),
        ("Shared".to_owned(), r#"{"space": "*"}"#.to_owned()),
    ];
    let spaces = Spaces::from_texts(&rules);
    // Under a folder of the space: nothing to say for it, the other space or none said.
    assert_eq!(spaces.says_for("Work", Some("work")), FolderSpace::Inherit);
    assert_eq!(spaces.says_for("Work", Some("home")), FolderSpace::In("home"));
    assert_eq!(spaces.says_for("Work", None), FolderSpace::None);
    // At the vault's top (in no space): none is what it has already.
    assert_eq!(spaces.says_for("", None), FolderSpace::Inherit);
    assert_eq!(spaces.says_for("", Some("home")), FolderSpace::In("home"));
    // Under a folder in every space, a folder says its own, whatever it is.
    assert_eq!(spaces.says_for("Shared", Some("work")), FolderSpace::In("work"));
    assert_eq!(spaces.says_for("Shared", None), FolderSpace::None);
    assert_eq!((spaces.says_for_every("Shared"), spaces.says_for_every("Work"), spaces.says_for_every("")), (FolderSpace::Inherit, FolderSpace::Shared, FolderSpace::Shared));
}
