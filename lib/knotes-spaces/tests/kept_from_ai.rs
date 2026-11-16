use knotes_spaces::{KeptFromAi, set_file_ai, set_folder_ai};
use serde_json::{Map, Value, json};

fn rules(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn the_nearest_setting_wins_a_note_s_tag_first() {
    let (private, shared, docs) = (rules(json!({"ai": false})), rules(json!({"ai": true})), rules(json!({"files": {"contract.pdf": {"ai": false}, "open.pdf": {"ai": true}}})));
    let private_files = rules(json!({"files": {"plan.pdf": {"ai": true}}}));
    let kept = KeptFromAi::new(["Diary.md"], [("Private", &private), ("Private/Shared", &shared), ("Docs", &docs), ("Private", &private_files)]);
    assert!(kept.holds("Diary.md"), "tagged");
    assert!(kept.holds("Private/Secret.md") && kept.holds("Private/Deep/x.md"), "a folder kept, its subfolders too");
    assert!(!kept.holds("Private/Shared/Plan.md"), "a subfolder shared again");
    assert!(kept.holds("Docs/contract.pdf") && !kept.holds("Docs/other.pdf"), "a file of its own");
    assert!(!kept.holds("Private/plan.pdf"), "a file shared in a folder kept");
    assert!(kept.holds_folder("Private") && kept.holds_folder("Private/Deep") && !kept.holds_folder("Private/Shared") && !kept.holds_folder("Docs"));
    assert_eq!((kept.own_folder("Private"), kept.own_folder("Private/Deep")), (Some(false), None));
    assert!(kept.holds("Мии\u{306}.md") == kept.holds("Мий.md"), "names compared composed");
}

#[test]
fn a_folder_and_a_file_get_their_own_setting_the_rest_of_the_file_kept() {
    let text = "{\n  \"space\": \"work\"\n}\n";
    let hidden = set_folder_ai(text, Some(false)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&hidden).unwrap(), json!({"space": "work", "ai": false}));
    assert_eq!(serde_json::from_str::<Value>(&set_folder_ai(&hidden, None).unwrap()).unwrap(), json!({"space": "work"}));
    let file = set_file_ai(text, "a.pdf", Some(false)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&file).unwrap(), json!({"space": "work", "files": {"a.pdf": {"ai": false}}}));
    assert_eq!(serde_json::from_str::<Value>(&set_file_ai(&file, "a.pdf", None).unwrap()).unwrap(), json!({"space": "work"}), "the last entry takes files with it");
    assert!(set_folder_ai("{ broken", Some(false)).is_err(), "a file that does not parse is not written over");
}
