use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use knotes_core::ChangeFeed;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

/// FSEvents watcher with debouncing: changes arriving within `debounce` of the first one
/// form one batch. Hidden paths (`.obsidian`, `.git`, dotfiles) are dropped.
/// Dropping the watcher stops the background thread.
pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    batches: Receiver<Vec<String>>,
}

impl FsWatcher {
    pub fn new(root: &Path, debounce: Duration) -> io::Result<Self> {
        // FSEvents reports real paths (/private/var/...), so compare against the canonical root.
        let root = root.canonicalize()?;
        let (raw_tx, raw_rx) = mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Ok(event) = event {
                for path in event.paths {
                    let _ = raw_tx.send(path);
                }
            }
        })
        .map_err(io::Error::other)?;
        watcher.watch(&root, RecursiveMode::Recursive).map_err(io::Error::other)?;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new().name("knotes-watch".into()).spawn(move || debounce_loop(&root, &raw_rx, &tx, debounce))?;
        Ok(Self { _watcher: watcher, batches: rx })
    }
}

impl ChangeFeed for FsWatcher {
    fn next_change(&self, timeout: Duration) -> Option<Vec<String>> {
        self.batches.recv_timeout(timeout).ok()
    }
}

fn debounce_loop(root: &Path, raw: &Receiver<PathBuf>, out: &Sender<Vec<String>>, debounce: Duration) {
    // `recv` fails once the watcher (and its callback holding the sender) is dropped.
    while let Ok(first) = raw.recv() {
        let mut paths = BTreeSet::new();
        add_visible(root, &first, &mut paths);
        let deadline = Instant::now() + debounce;
        loop {
            match raw.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(path) => add_visible(root, &path, &mut paths),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if !paths.is_empty() && out.send(paths.into_iter().collect()).is_err() {
            return;
        }
    }
}

fn add_visible(root: &Path, path: &Path, paths: &mut BTreeSet<String>) {
    let Some(rel) = path.strip_prefix(root).ok().and_then(Path::to_str) else { return };
    if !rel.is_empty() && !rel.split('/').any(|part| part.starts_with('.')) {
        paths.insert(rel.to_owned());
    }
}
