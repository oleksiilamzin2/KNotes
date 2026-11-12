use std::fs;

use knotes_core::FolderRulesStore;
use knotes_vault::FsVault;

#[test]
fn reads_existing_rules_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("Journal/2026")).unwrap();
    fs::write(r.join(".knotes"), "{\"list\":{}}").unwrap();
    fs::write(r.join("Journal/.knotes"), "{}").unwrap();
    let found = FsVault::new(r).read_rules(&["", "Journal", "Journal/2026", "Missing"]).unwrap();
    assert_eq!(found, [("".to_owned(), "{\"list\":{}}".to_owned()), ("Journal".to_owned(), "{}".to_owned())]);
}

#[test]
fn writes_rules_atomically() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Journal")).unwrap();
    let vault = FsVault::new(dir.path());
    vault.write_rules("Journal", "{\"a\":1}\n").unwrap();
    vault.write_rules("Journal", "{\"a\":2}\n").unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("Journal/.knotes")).unwrap(), "{\"a\":2}\n");
    let names: Vec<_> = fs::read_dir(dir.path().join("Journal")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, [".knotes"]);
}
