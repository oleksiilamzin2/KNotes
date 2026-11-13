use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use knotes_core::ChangeFeed;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

/// FSEvents watcher with debouncing: changes arriving within `debounce` of the first one
/// form one batch. Hidden paths (`.obsidian`, `.git`, dotfiles) are dropped, except the
/// per-folder rules files `.knotes.json`.
/// Dropping the watcher stops the background thread.
pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    batches: Option<Receiver<Vec<String>>>,
}

impl FsWatcher {
    pub fn new(root: &Path, debounce: Duration) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = Self::with_callback(root, debounce, move |batch| tx.send(batch).is_ok())?;
        watcher.batches = Some(rx);
        Ok(watcher)
    }

    /// Calls `on_batch` from the watcher thread for each debounced batch, so a consumer can
    /// block on its own channel together with other messages. `on_batch` returns `false` to
    /// stop watching. `next_change` then never reports anything.
    pub fn with_callback(root: &Path, debounce: Duration, on_batch: impl Fn(Vec<String>) -> bool + Send + 'static) -> io::Result<Self> {
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
        thread::Builder::new().name("knotes-watch".into()).spawn(move || debounce_loop(&root, &raw_rx, &on_batch, debounce))?;
        Ok(Self { _watcher: watcher, batches: None })
    }
}

impl ChangeFeed for FsWatcher {
    fn next_change(&self, timeout: Duration) -> Option<Vec<String>> {
        match &self.batches {
            Some(rx) => rx.recv_timeout(timeout).ok(),
            None => {
                thread::sleep(timeout);
                None
            }
        }
    }
}

fn debounce_loop(root: &Path, raw: &Receiver<PathBuf>, on_batch: &impl Fn(Vec<String>) -> bool, debounce: Duration) {
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
        if !paths.is_empty() && !on_batch(paths.into_iter().collect()) {
            return;
        }
    }
}

/// Keeps visible paths and per-folder rules files (`.knotes.json` and the old `.knotes`, ADR
/// 0010, 0020); drops anything else hidden.
fn add_visible(root: &Path, path: &Path, paths: &mut BTreeSet<String>) {
    let Some(rel) = path.strip_prefix(root).ok().and_then(Path::to_str) else { return };
    let (folders, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let hidden_folder = folders.split('/').any(|part| part.starts_with('.'));
    if !rel.is_empty() && !hidden_folder && (!name.starts_with('.') || name == ".knotes.json" || name == ".knotes") {
        paths.insert(rel.to_owned());
    }
}
