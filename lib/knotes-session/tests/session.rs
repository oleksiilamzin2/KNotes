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
