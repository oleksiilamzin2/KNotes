use std::fs;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use knotes_session::{Event, Snapshot, VaultSession, index_dir};

fn wait_for(events: &Receiver<Event>, mut done: impl FnMut(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
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
    fs::write(vault.path().join("Projects/.knotes"), r#"{"list": {"sort": "title"}}"#).unwrap();
    let (_session, events) = VaultSession::open(vault.path(), data.path()).unwrap();
    let first = wait_for(&events, |s| !s.rows.is_empty());
    assert_eq!(resolve_list_settings("Projects", &first.rules).sort, SortOrder::Title);
    fs::write(vault.path().join("Projects/.knotes"), r#"{"list": {"sort": "modified"}}"#).unwrap();
    wait_for(&events, |s| resolve_list_settings("Projects", &s.rules).sort == SortOrder::Modified);
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
    assert!(start.elapsed() < Duration::from_millis(100), "{:?}", start.elapsed());
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
