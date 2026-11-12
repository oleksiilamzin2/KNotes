//! Launches the real app in probe mode: it builds the window, prints its structure and quits.

use std::process::Command;
use std::time::{Duration, Instant};

fn probe() -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes"))
        .env("KNOTES_PROBE", "1")
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
