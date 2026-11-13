use std::fs;

use knotes_core::AttachmentStore;
use knotes_vault::FsVault;

#[test]
fn an_attachment_goes_into_the_knotes_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = FsVault::new(dir.path()).add_attachment("Pasted image 20261113114500.png", b"png").unwrap();
    assert_eq!(path, ".knotes/attachments/Pasted image 20261113114500.png");
    assert_eq!(fs::read(dir.path().join(&path)).unwrap(), b"png");
}

#[test]
fn a_taken_name_gets_a_number_and_nothing_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let vault = FsVault::new(dir.path());
    assert_eq!(vault.add_attachment("photo.jpg", b"1").unwrap(), ".knotes/attachments/photo.jpg");
    assert_eq!(vault.add_attachment("photo.jpg", b"2").unwrap(), ".knotes/attachments/photo 1.jpg");
    assert_eq!(vault.add_attachment("photo.jpg", b"3").unwrap(), ".knotes/attachments/photo 2.jpg");
    assert_eq!(fs::read(dir.path().join(".knotes/attachments/photo.jpg")).unwrap(), b"1");
    let names = fs::read_dir(dir.path().join(".knotes/attachments")).unwrap().count();
    assert_eq!(names, 3, "no temporary file left");
}

#[test]
fn an_old_rules_file_at_the_root_makes_way_for_the_knotes_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".knotes"), "{\"list\":{}}").unwrap();
    FsVault::new(dir.path()).add_attachment("a.png", b"png").unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(".knotes.json")).unwrap(), "{\"list\":{}}");
    assert!(dir.path().join(".knotes/attachments/a.png").is_file());
}

#[test]
fn a_name_with_a_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(FsVault::new(dir.path()).add_attachment("../x.png", b"png").is_err());
}
