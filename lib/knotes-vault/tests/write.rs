use std::fs;

use knotes_core::NoteWriter;
use knotes_vault::FsVault;

#[test]
fn writes_replace_notes_atomically_and_create_folders() {
    let dir = tempfile::tempdir().unwrap();
    let vault = FsVault::new(dir.path());
    vault.write_note("Нове/Тека/n.md", "перша").unwrap();
    vault.write_note("Нове/Тека/n.md", "друга версія").unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("Нове/Тека/n.md")).unwrap(), "друга версія");
    let names: Vec<_> = fs::read_dir(dir.path().join("Нове/Тека")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["n.md"], "no temporary files left");
}

#[test]
fn paths_cannot_leave_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let vault = FsVault::new(dir.path().join("vault"));
    assert!(vault.write_note("../outside.md", "x").is_err());
    assert!(vault.write_note("/abs.md", "x").is_err());
    assert!(!dir.path().join("outside.md").exists());
}
