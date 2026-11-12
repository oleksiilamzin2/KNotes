use std::fs;

use knotes_core::NoteMover;
use knotes_vault::FsVault;

#[test]
fn notes_move_into_new_folders_and_never_over_another_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "a").unwrap();
    fs::write(dir.path().join("b.md"), "b").unwrap();
    let vault = FsVault::new(dir.path());
    vault.move_note("a.md", "Нова/Тека/Назва.md").unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("Нова/Тека/Назва.md")).unwrap(), "a");
    assert!(!dir.path().join("a.md").exists());
    let err = vault.move_note("b.md", "Нова/Тека/Назва.md").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(dir.path().join("Нова/Тека/Назва.md")).unwrap(), "a", "the target is untouched");
    assert!(vault.move_note("b.md", "../out.md").is_err());
}

#[test]
fn a_change_of_letter_case_renames_the_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "n").unwrap();
    FsVault::new(dir.path()).move_note("note.md", "Note.md").unwrap();
    let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(names, ["Note.md"]);
}
