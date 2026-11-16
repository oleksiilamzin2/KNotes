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

#[cfg(target_os = "macos")]
#[test]
fn a_rewritten_note_keeps_its_permissions_creation_date_and_extended_attributes() {
    use std::os::macos::fs::FileTimesExt;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("n.md");
    fs::write(&file, "old").unwrap();
    // 2020-01-01, a private note, and a tag (Finder keeps its tags in extended attributes).
    let created = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_577_880_000);
    fs::File::options().write(true).open(&file).unwrap().set_times(fs::FileTimes::new().set_created(created)).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(Command::new("xattr").args(["-w", "com.example.tag", "red"]).arg(&file).status().unwrap().success());
    FsVault::new(dir.path()).write_note("n.md", "new").unwrap();
    let meta = fs::metadata(&file).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    assert_eq!(meta.created().unwrap(), created);
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let tag = Command::new("xattr").args(["-p", "com.example.tag"]).arg(&file).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&tag.stdout).trim(), "red");
    assert!(meta.modified().unwrap() > created, "the modification time is the write's");
}
