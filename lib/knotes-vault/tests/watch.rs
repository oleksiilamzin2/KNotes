use std::fs;
use std::time::Duration;

use knotes_core::ChangeFeed;
use knotes_vault::FsWatcher;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn reports_changed_paths_in_one_debounced_batch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join("Folder")).unwrap();
    let watcher = FsWatcher::new(&root, Duration::from_millis(200)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(root.join("a.md"), "one").unwrap();
    fs::write(root.join("Folder/b.md"), "two").unwrap();
    fs::write(root.join("a.md"), "one again").unwrap();
    let mut seen = Vec::new();
    while let Some(batch) = watcher.next_change(WAIT) {
        seen.extend(batch);
        if seen.contains(&"a.md".to_owned()) && seen.contains(&"Folder/b.md".to_owned()) {
            break;
        }
    }
    assert!(seen.contains(&"a.md".to_owned()) && seen.contains(&"Folder/b.md".to_owned()), "{seen:?}");
}

#[test]
fn hidden_folders_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join(".obsidian")).unwrap();
    let watcher = FsWatcher::new(&root, Duration::from_millis(100)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(root.join(".obsidian/workspace.json"), "{}").unwrap();
    assert_eq!(watcher.next_change(Duration::from_millis(1500)), None);
}

#[test]
fn folder_rules_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join("Journal")).unwrap();
    let watcher = FsWatcher::new(&root, Duration::from_millis(100)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(root.join("Journal/.knotes.json"), "{}").unwrap();
    let mut seen = Vec::new();
    while let Some(batch) = watcher.next_change(WAIT) {
        seen.extend(batch);
        if seen.contains(&"Journal/.knotes.json".to_owned()) {
            break;
        }
    }
    assert!(seen.contains(&"Journal/.knotes.json".to_owned()), "{seen:?}");
}

#[test]
fn attachments_in_the_knotes_folder_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join(".knotes/attachments")).unwrap();
    let watcher = FsWatcher::new(&root, Duration::from_millis(100)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(root.join(".knotes/attachments/pic.png"), "png").unwrap();
    fs::write(root.join(".knotes/other.json"), "{}").unwrap();
    let mut seen = Vec::new();
    while let Some(batch) = watcher.next_change(WAIT) {
        seen.extend(batch);
        if seen.contains(&".knotes/attachments/pic.png".to_owned()) {
            break;
        }
    }
    assert!(seen.contains(&".knotes/attachments/pic.png".to_owned()), "{seen:?}");
    assert!(!seen.contains(&".knotes/other.json".to_owned()), "{seen:?}");
}
