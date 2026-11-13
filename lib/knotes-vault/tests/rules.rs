use std::fs;

use knotes_core::FolderRulesStore;
use knotes_vault::FsVault;

#[test]
fn reads_existing_rules_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("Journal/2026")).unwrap();
    fs::write(r.join(".knotes.json"), "{\"list\":{}}").unwrap();
    fs::write(r.join("Journal/.knotes.json"), "{}").unwrap();
    let found = FsVault::new(r).read_rules(&["", "Journal", "Journal/2026", "Missing"]).unwrap();
    assert_eq!(found, [("".to_owned(), "{\"list\":{}}".to_owned()), ("Journal".to_owned(), "{}".to_owned())]);
}

#[test]
fn an_old_rules_file_is_read_while_there_is_no_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("Journal")).unwrap();
    fs::write(r.join("Journal/.knotes"), "{\"old\":1}").unwrap();
    fs::write(r.join(".knotes"), "{\"old\":2}").unwrap();
    fs::write(r.join(".knotes.json"), "{\"new\":2}").unwrap();
    let found = FsVault::new(r).read_rules(&["", "Journal"]).unwrap();
    assert_eq!(found, [("".to_owned(), "{\"new\":2}".to_owned()), ("Journal".to_owned(), "{\"old\":1}".to_owned())]);
}

#[test]
fn the_knotes_folder_at_the_root_is_no_rules_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".knotes/attachments")).unwrap();
    assert_eq!(FsVault::new(dir.path()).read_rules(&[""]).unwrap(), []);
}

#[test]
fn writes_rules_atomically() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Journal")).unwrap();
    let vault = FsVault::new(dir.path());
    vault.write_rules("Journal", "{\"a\":1}\n").unwrap();
    vault.write_rules("Journal", "{\"a\":2}\n").unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("Journal/.knotes.json")).unwrap(), "{\"a\":2}\n");
    let names: Vec<_> = fs::read_dir(dir.path().join("Journal")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, [".knotes.json"]);
}

#[test]
fn writing_rules_replaces_an_old_rules_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".knotes"), "{\"old\":1}").unwrap();
    FsVault::new(dir.path()).write_rules("", "{\"a\":1}\n").unwrap();
    let mut names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    assert_eq!(names, [".knotes.json"]);
}
