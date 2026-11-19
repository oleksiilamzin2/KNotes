use std::fs;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use knotes_session::{Event, Snapshot, VaultSession, index_dir};

fn wait_for(events: &Receiver<Event>, mut done: impl FnMut(&Snapshot) -> bool) -> Snapshot {
    // A minute, not the ten seconds it was: these tests run together, a dozen sessions in one
    // process, and their changes are then seen one after another, half a second each (measured:
    // 0.6 s for a wait alone, up to 7 s together). On a busy Mac the last ones ran out of
    // their ten seconds, and a run failed by chance. Waiting longer costs nothing when the
    // snapshot comes.
    let deadline = Instant::now() + Duration::from_secs(60);
    while let Ok(event) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        match event {
            Event::Snapshot(s) if done(&s) => return s,
            Event::Error(e) => panic!("session error: {e}"),
            _ => {}
        }
    }
    panic!("no matching snapshot in time");
}

#[test]
fn index_dir_is_stable_per_vault_and_outside_it() {
    let data = tempfile::tempdir().unwrap();
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert_eq!(index_dir(data.path(), a.path()), index_dir(data.path(), a.path()));
    assert_ne!(index_dir(data.path(), a.path()), index_dir(data.path(), b.path()));
    assert!(index_dir(data.path(), a.path()).starts_with(data.path().join("vaults")));
}

#[test]
fn opening_a_vault_delivers_snapshots_and_follows_changes() {
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir(vault.path().join("Projects")).unwrap();
    fs::write(vault.path().join("Projects/a.md"), "#radar first").unwrap();
    fs::write(vault.path().join("b.md"), "second").unwrap();
    let (session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 2);
    assert_eq!(first.folders.notes, 2);
    assert_eq!(first.tags.iter().map(|t| t.tag.as_str()).collect::<Vec<_>>(), ["radar"]);

    fs::write(vault.path().join("Projects/c.md"), "third").unwrap();
    let later = wait_for(&events, |s| s.rows.len() == 3);
    assert!(later.changed.iter().any(|p| p == "Projects/c.md"), "{:?}", later.changed);
    drop(session);
}

#[test]
fn a_vault_opens_only_once() {
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (first, _events) = VaultSession::open(vault.path(), data.path()).unwrap();
    assert!(VaultSession::open(vault.path(), data.path()).is_err());
    drop(first);
    assert!(VaultSession::open(vault.path(), data.path()).is_ok());
}

#[test]
fn snapshots_carry_folder_rules_and_follow_their_changes() {
    use knotes_app::{SortOrder, resolve_list_settings};
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir(vault.path().join("Projects")).unwrap();
    fs::write(vault.path().join("Projects/a.md"), "a").unwrap();
    fs::write(vault.path().join("Projects/.knotes.json"), r#"{"list": {"sort": "title"}}"#).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| !s.rows.is_empty());
    assert_eq!(resolve_list_settings("Projects", &first.rules).sort, SortOrder::Title);
    fs::write(vault.path().join("Projects/.knotes.json"), r#"{"list": {"sort": "modified"}}"#).unwrap();
    wait_for(&events, |s| resolve_list_settings("Projects", &s.rules).sort == SortOrder::Modified);
}

#[test]
fn snapshots_carry_the_spaces_and_follow_their_changes() {
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir_all(vault.path().join("Work/Radar")).unwrap();
    fs::write(vault.path().join("Work/Radar/a.md"), "a").unwrap();
    fs::write(vault.path().join("b.md"), "b").unwrap();
    fs::write(vault.path().join(".knotes.json"), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}]}"#).unwrap();
    fs::write(vault.path().join("Work/.knotes.json"), r#"{"space": "work"}"#).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 2);
    assert_eq!(first.spaces.list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Робота"]);
    assert_eq!(first.spaces.of("Work/Radar/a.md").map(|s| s.id.as_str()), Some("work"), "a subfolder's note");
    assert_eq!(first.spaces.of("b.md"), None);
    fs::write(vault.path().join("Work/.knotes.json"), r#"{"space": null}"#).unwrap();
    wait_for(&events, |s| s.spaces.of("Work/Radar/a.md").is_none());
}

#[test]
fn snapshots_show_empty_folders_and_folders_made_meanwhile() {
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir_all(vault.path().join("Empty/Inner")).unwrap();
    fs::create_dir(vault.path().join(".hidden")).unwrap();
    fs::write(vault.path().join("a.md"), "a").unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let names = |s: &Snapshot| s.folders.children.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let first = wait_for(&events, |s| s.rows.len() == 1);
    assert_eq!(names(&first), ["Empty"], "hidden folders left out");
    assert_eq!(first.folders.children[0].children[0].name, "Inner");
    fs::create_dir(vault.path().join("Made")).unwrap();
    wait_for(&events, |s| names(s).contains(&"Made".to_string()));
    fs::remove_dir_all(vault.path().join("Empty")).unwrap();
    wait_for(&events, |s| !names(s).contains(&"Empty".to_string()));
}

#[test]
fn snapshots_carry_the_recently_deleted_and_the_vault_s_days_purge_the_older() {
    use knotes_core::TrashStore;
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for name in ["old.md", "new.md", "a.md"] {
        fs::write(vault.path().join(name), name).unwrap();
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let day = 86_400_000;
    let files = knotes_vault::FsVault::new(vault.path());
    files.trash("old.md", now - 40 * day).unwrap();
    files.trash("new.md", now - day).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 1 && s.trash.len() == 1);
    assert_eq!(first.trash[0].path, "new.md", "the one older than 30 days gone for good");
    assert_eq!(files.trashed().unwrap().len(), 1);
}

#[test]
fn closing_a_session_is_immediate() {
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::write(vault.path().join("a.md"), "a").unwrap();
    let (session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    wait_for(&events, |s| s.rows.len() == 1);
    std::thread::sleep(Duration::from_millis(50));
    let start = Instant::now();
    drop(session);
    // Closing must not wait for the 2 s maintenance timer. Closing SQLite checkpoints the WAL
    // with an fsync, which took up to ~300 ms with the other session tests writing in
    // parallel, so the bound leaves room for disk noise.
    assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
}

#[test]
fn snapshots_resolve_links_and_search_titles_and_reuse_them_while_files_stay() {
    use knotes_core::{LinkKind, LinkResolver, TitleSearch};
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir(vault.path().join("Projects")).unwrap();
    fs::write(vault.path().join("Projects/Radar.md"), "a").unwrap();
    fs::write(vault.path().join("b.md"), "b").unwrap();
    let (session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 2);
    assert_eq!(first.links.resolve(LinkKind::Wiki, "radar", "b.md").as_deref(), Some("Projects/Radar.md"));
    assert_eq!(first.titles.search_titles("rad", 5)[0].path, "Projects/Radar.md");

    fs::write(vault.path().join("b.md"), "edited").unwrap();
    let edited = wait_for(&events, |s| s.changed.iter().any(|p| p == "b.md"));
    assert!(std::sync::Arc::ptr_eq(&first.links, &edited.links), "same files: no rebuild");
    assert!(std::sync::Arc::ptr_eq(&first.titles, &edited.titles));

    fs::write(vault.path().join("New.md"), "c").unwrap();
    let added = wait_for(&events, |s| s.rows.len() == 3);
    assert_eq!(added.links.resolve(LinkKind::Wiki, "new", "b.md").as_deref(), Some("New.md"));
    assert_eq!(added.titles.search_titles("new", 5)[0].path, "New.md");
    drop(session);
}

#[test]
fn snapshots_carry_each_tag_with_its_note_and_the_untagged_and_reuse_them_while_the_tags_stay() {
    use std::sync::Arc;
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::write(vault.path().join("a.md"), "#radar #area/job one").unwrap();
    fs::write(vault.path().join("b.md"), "two").unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 2);
    let mut tagged = first.tagged.to_vec();
    tagged.sort();
    assert_eq!(tagged, [("area/job".to_owned(), "a.md".to_owned()), ("radar".to_owned(), "a.md".to_owned())]);
    assert_eq!(first.untagged.iter().collect::<Vec<_>>(), ["b.md"]);
    assert_eq!(first.tags.iter().map(|t| (t.tag.as_str(), t.notes)).collect::<Vec<_>>(), [("area", 1), ("radar", 1)]);
    fs::write(vault.path().join("b.md"), "two, longer").unwrap();
    let later = wait_for(&events, |s| s.changed.iter().any(|p| p == "b.md"));
    assert!(Arc::ptr_eq(&first.tagged, &later.tagged) && Arc::ptr_eq(&first.tags, &later.tags) && Arc::ptr_eq(&first.untagged, &later.untagged), "the same tags: the same lookups");
    fs::write(vault.path().join("b.md"), "#radar two").unwrap();
    let tagged = wait_for(&events, |s| s.untagged.is_empty());
    assert_eq!(tagged.tags.iter().map(|t| (t.tag.as_str(), t.notes)).collect::<Vec<_>>(), [("area", 1), ("radar", 2)]);
}

#[test]
fn fewer_days_set_while_the_vault_is_open_purge_at_once_and_snapshots_keep_the_rest_as_they_were() {
    use knotes_core::TrashStore;
    use std::sync::Arc;
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for name in ["old.md", "new.md", "a.md"] {
        fs::write(vault.path().join(name), name).unwrap();
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let day = 86_400_000;
    let files = knotes_vault::FsVault::new(vault.path());
    files.trash("old.md", now - 10 * day).unwrap();
    files.trash("new.md", now - day).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 1 && s.trash.len() == 2);
    fs::write(vault.path().join("a.md"), "a, longer").unwrap();
    let later = wait_for(&events, |s| s.changed.iter().any(|p| p == "a.md"));
    assert!(Arc::ptr_eq(&first.trash, &later.trash), "the same entries: the same list");
    fs::write(vault.path().join(".knotes.json"), r#"{"trash": {"days": 5}}"#).unwrap();
    let purged = wait_for(&events, |s| s.trash.len() == 1);
    assert_eq!(purged.trash[0].path, "new.md");
}

#[test]
fn snapshots_count_each_space_s_tags_and_untagged_notes_and_count_again_only_when_they_change() {
    use std::sync::Arc;
    let (vault, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::create_dir_all(vault.path().join("Work")).unwrap();
    fs::write(vault.path().join("Work/a.md"), "#radar #area/job a").unwrap();
    fs::write(vault.path().join("Work/b.md"), "plain").unwrap();
    fs::write(vault.path().join("top.md"), "#cooking #radar top").unwrap();
    fs::write(vault.path().join(".knotes.json"), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}]}"#).unwrap();
    fs::write(vault.path().join("Work/.knotes.json"), r#"{"space": "work"}"#).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| s.rows.len() == 3);
    let counted = |nodes: &[knotes_app::TagNode]| nodes.iter().map(|t| (t.tag.clone(), t.notes)).collect::<Vec<_>>();
    let work = &first.space_tags[0];
    assert_eq!(work.id, "work");
    assert_eq!(counted(&work.tags), [("area".to_owned(), 1), ("radar".to_owned(), 1)], "counted within the space");
    assert_eq!(counted(&work.rest), [("cooking".to_owned(), 1)], "only outside it");
    assert_eq!(work.untagged, 1);
    fs::write(vault.path().join("Work/b.md"), "plain, longer").unwrap();
    let later = wait_for(&events, |s| s.changed.iter().any(|p| p == "Work/b.md"));
    assert!(Arc::ptr_eq(&first.space_tags, &later.space_tags), "the same tags: not counted again");
    fs::write(vault.path().join("Work/b.md"), "#cooking now").unwrap();
    let tagged = wait_for(&events, |s| s.space_tags[0].untagged == 0);
    assert_eq!(counted(&tagged.space_tags[0].tags), [("area".to_owned(), 1), ("cooking".to_owned(), 1), ("radar".to_owned(), 1)]);
    assert!(tagged.space_tags[0].rest.is_empty());
}
