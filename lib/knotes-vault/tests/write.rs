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

#[test]
fn a_note_not_in_utf8_is_read_exactly_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let vault = FsVault::new(dir.path());
    // "Привіт" in Windows-1251.
    fs::write(dir.path().join("old.md"), [0xcf, 0xf0, 0xe8, 0xe2, 0xb3, 0xf2]).unwrap();
    fs::write(dir.path().join("new.md"), "Привіт").unwrap();
    assert_eq!(vault.read_utf8("new.md").unwrap(), "Привіт");
    assert_eq!(vault.read_utf8("old.md").unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(vault.read_utf8("none.md").unwrap_err().kind(), std::io::ErrorKind::NotFound);
}
