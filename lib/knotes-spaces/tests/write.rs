use knotes_spaces::{FolderSpace, Space, SpaceColor, Spaces, new_space_id, set_folder_space, set_spaces};
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
