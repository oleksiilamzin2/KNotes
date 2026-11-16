use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use knotes_core::TrashStore;
use knotes_vault::FsVault;

const NOW: i64 = 1_794_000_000_000;

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

#[test]
fn a_deleted_note_is_kept_as_it_was_and_comes_back_where_it_was() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Work/Plan.md", "---\ntags: [plan]\n---\nThe plan.");
    let old = SystemTime::now() - Duration::from_secs(86_400 * 3);
    fs::File::options().write(true).open(dir.path().join("Work/Plan.md")).unwrap().set_modified(old).unwrap();
    let vault = FsVault::new(dir.path());
    let entry = vault.trash("Work/Plan.md", NOW).unwrap();
    assert_eq!((entry.path.as_str(), entry.folder, entry.deleted_ms, entry.notes), ("Work/Plan.md", false, NOW, 1));
    assert!(!dir.path().join("Work/Plan.md").exists(), "out of the vault's notes");
    assert!(dir.path().join(".knotes/trash").is_dir(), "kept in the vault's hidden folder");
    assert_eq!(vault.trashed().unwrap(), std::slice::from_ref(&entry));
    assert_eq!(vault.restore(&entry.id).unwrap(), "Work/Plan.md");
    assert_eq!(fs::read_to_string(dir.path().join("Work/Plan.md")).unwrap(), "---\ntags: [plan]\n---\nThe plan.", "its frontmatter with it");
    assert_eq!(fs::metadata(dir.path().join("Work/Plan.md")).unwrap().modified().unwrap(), old, "and its date");
    assert!(vault.trashed().unwrap().is_empty());
}

#[test]
fn a_deleted_folder_goes_whole_with_its_rules_and_comes_back_whole() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Work/.knotes.json", r#"{"space": "work"}"#);
    write(dir.path(), "Work/a.md", "a");
    write(dir.path(), "Work/Radar/.knotes.json", r#"{"list": {"sort": "title"}}"#);
    write(dir.path(), "Work/Radar/b.md", "b");
    write(dir.path(), "Work/Radar/doc.pdf", "%PDF");
    let vault = FsVault::new(dir.path());
    let entry = vault.trash("Work", NOW).unwrap();
    assert!(entry.folder && entry.notes == 2, "{entry:?}");
    assert!(!dir.path().join("Work").exists());
    vault.restore(&entry.id).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("Work/.knotes.json")).unwrap(), r#"{"space": "work"}"#);
    assert_eq!(fs::read_to_string(dir.path().join("Work/Radar/.knotes.json")).unwrap(), r#"{"list": {"sort": "title"}}"#);
    assert!(dir.path().join("Work/Radar/b.md").exists() && dir.path().join("Work/Radar/doc.pdf").exists());
}

#[test]
fn a_note_restored_after_its_folder_went_brings_the_folder_back_with_its_rules() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Work/.knotes.json", r#"{"space": "work"}"#);
    write(dir.path(), "Work/Radar/.knotes.json", r#"{"list": {"sort": "title"}}"#);
    write(dir.path(), "Work/Radar/b.md", "b");
    let vault = FsVault::new(dir.path());
    let entry = vault.trash("Work/Radar/b.md", NOW).unwrap();
    fs::remove_dir_all(dir.path().join("Work")).unwrap();
    assert_eq!(vault.restore(&entry.id).unwrap(), "Work/Radar/b.md");
    assert_eq!(fs::read_to_string(dir.path().join("Work/.knotes.json")).unwrap(), r#"{"space": "work"}"#, "the folders above made again as they were");
    assert_eq!(fs::read_to_string(dir.path().join("Work/Radar/.knotes.json")).unwrap(), r#"{"list": {"sort": "title"}}"#);
}

#[test]
fn a_restore_onto_a_taken_place_takes_another_name_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Plan.md", "old");
    write(dir.path(), "Archive/x.md", "x");
    let vault = FsVault::new(dir.path());
    let note = vault.trash("Plan.md", NOW).unwrap();
    let folder = vault.trash("Archive", NOW + 1).unwrap();
    write(dir.path(), "Plan.md", "new");
    write(dir.path(), "Archive/y.md", "y");
    assert_eq!(vault.restore(&note.id).unwrap(), "Plan 2.md");
    assert_eq!(fs::read_to_string(dir.path().join("Plan 2.md")).unwrap(), "old");
    assert_eq!(fs::read_to_string(dir.path().join("Plan.md")).unwrap(), "new", "the new one untouched");
    assert_eq!(vault.restore(&folder.id).unwrap(), "Archive 2");
    assert!(dir.path().join("Archive 2/x.md").exists());
}

#[test]
fn entries_list_newest_first_and_go_for_good_when_purged() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.md", "a");
    write(dir.path(), "b.md", "b");
    let vault = FsVault::new(dir.path());
    let a = vault.trash("a.md", NOW).unwrap();
    let b = vault.trash("b.md", NOW + 5).unwrap();
    assert_eq!(vault.trashed().unwrap().iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), ["b.md", "a.md"]);
    vault.purge(&a.id).unwrap();
    assert_eq!(vault.trashed().unwrap(), [b]);
    assert!(vault.restore(&a.id).is_err(), "gone for good");
    assert!(vault.trash("../outside.md", NOW).is_err() && vault.trash("missing.md", NOW).is_err());
}

#[test]
fn an_entry_says_where_it_is_kept_so_it_can_be_read_there() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Work/Plan.md", "The plan.");
    let vault = FsVault::new(dir.path());
    let entry = vault.trash("Work/Plan.md", NOW).unwrap();
    let kept = vault.kept_at(&entry.id).unwrap();
    assert_eq!(fs::read_to_string(&kept).unwrap(), "The plan.");
    assert!(kept.starts_with(dir.path().join(".knotes/trash")), "{}", kept.display());
    assert!(vault.kept_at("../../etc").is_err());
}

#[test]
fn entries_already_known_are_not_read_again_only_the_new_and_the_gone_change_the_list() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.md", "b.md", "c.md"] {
        write(dir.path(), name, name);
    }
    let vault = FsVault::new(dir.path());
    let a = vault.trash("a.md", NOW).unwrap();
    let b = vault.trash("b.md", NOW + 1).unwrap();
    let known = vault.trashed().unwrap();
    // Read again, `a` would be skipped: it is not, being known.
    fs::write(dir.path().join(".knotes/trash").join(&a.id).join("entry.json"), "not json").unwrap();
    vault.purge(&b.id).unwrap();
    let c = vault.trash("c.md", NOW + 2).unwrap();
    assert_eq!(vault.trashed_with(&known).unwrap(), [c, a], "the new read, the gone dropped, the known kept; the newest first");
}
