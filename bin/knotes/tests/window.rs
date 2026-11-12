//! Launches the real app in probe mode: it builds the window, prints its structure and quits.

use std::process::Command;
use std::time::{Duration, Instant};

fn probe() -> String {
    probe_with(&[], None)
}

/// Runs the app in probe mode with a throwaway HOME, so no real settings or index are touched.
fn probe_with(args: &[&std::path::Path], home: Option<&std::path::Path>) -> String {
    let scratch_home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes"))
        .args(args)
        .env("KNOTES_PROBE", "1")
        .env("HOME", home.unwrap_or(scratch_home.path()))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("app starts");
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut out = String::new();
            std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut out).unwrap();
            assert!(status.success(), "app failed: {out}");
            return out;
        }
        if start.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("app did not quit in probe mode");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn window_has_four_columns_with_native_roles() {
    let out = probe();
    assert!(out.contains("columns: sidebar, content-list, content, inspector"), "{out}");
    assert!(out.contains("title: Knotes"), "{out}");
}

#[test]
fn window_opens_at_a_usable_size() {
    let out = probe();
    let size = out.lines().find_map(|l| l.strip_prefix("size: ")).expect("size line");
    let (w, h) = size.split_once('x').unwrap();
    let (w, h): (f64, f64) = (w.parse().unwrap(), h.parse().unwrap());
    assert!(w >= 1000.0 && h >= 600.0, "{size}");
}

#[test]
fn sidebar_shows_all_notes_folders_and_tags_of_the_vault() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Projects/Radar")).unwrap();
    std::fs::create_dir_all(vault.path().join("Inbox")).unwrap();
    std::fs::write(vault.path().join("Projects/Radar/a.md"), "#radar one").unwrap();
    std::fs::write(vault.path().join("Projects/b.md"), "two").unwrap();
    std::fs::write(vault.path().join("Inbox/c.md"), "three #todo").unwrap();
    let out = probe_with(&[vault.path()], None);
    let sidebar = out.lines().find_map(|l| l.strip_prefix("sidebar: ")).unwrap_or_else(|| panic!("{out}"));
    assert_eq!(sidebar, "Усі нотатки (3) | Теки | Inbox (1) | Projects (2) | Теги | radar (1) | todo (1)");
    let size = out.lines().find_map(|l| l.strip_prefix("size: ")).expect("size line");
    let width: f64 = size.split_once('x').unwrap().0.parse().unwrap();
    assert!(width >= 1000.0, "window shrank after data arrived: {size}");
}
