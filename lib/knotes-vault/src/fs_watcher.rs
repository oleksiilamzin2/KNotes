use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use knotes_core::{ATTACHMENTS_FOLDER, ChangeFeed};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

/// FSEvents watcher with debouncing: changes arriving within `debounce` of the first one
/// form one batch. Hidden paths (`.obsidian`, `.git`, dotfiles) are dropped, except the
/// per-folder rules files `.knotes.json`. When FSEvents says events were lost and the tree must
/// be scanned again (`RESCAN`), the batch holds `RESCAN`: anything may have changed.
/// Dropping the watcher stops the background thread, and returns at once: FSEvents is stopped
/// on a thread of its own. Stopping spins until the stream's run loop idles and asks fseventsd,
/// which answers its clients in turn and took seconds when they were many and slow (tests run
/// from a folder of half a million build files); a vault closing need not wait for it.
pub const RESCAN: &str = "";

pub struct FsWatcher {
    watcher: Option<RecommendedWatcher>,
    batches: Option<Receiver<Vec<String>>>,
}

impl Drop for FsWatcher {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            // Should no thread start, the closure, and the watcher with it, goes here.
            let _ = thread::Builder::new().name("knotes-watch-stop".into()).spawn(move || drop(watcher));
        }
    }
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
        let watched = root.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Ok(event) = event {
                // Events were dropped (a burst, a full queue): the root stands for them all.
                if event.need_rescan() {
                    let _ = raw_tx.send(watched.clone());
                }
                for path in event.paths {
                    let _ = raw_tx.send(path);
                }
            }
        })
        .map_err(io::Error::other)?;
        watcher.watch(&root, RecursiveMode::Recursive).map_err(io::Error::other)?;
        thread::Builder::new().name("knotes-watch".into()).spawn(move || debounce_loop(&root, &raw_rx, &on_batch, debounce))?;
        Ok(Self { watcher: Some(watcher), batches: None })
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

/// Keeps visible paths, the attachments folder's files, per-folder rules files (`.knotes.json`
/// and the old `.knotes`, ADR 0010, 0020), and the cards' files in `.knotes` (ADR 0037: the
/// review logs and the decks' settings, which the index takes in at a sync); drops anything
/// else hidden. The root itself is `RESCAN`.
fn add_visible(root: &Path, path: &Path, paths: &mut BTreeSet<String>) {
    let Some(rel) = path.strip_prefix(root).ok().and_then(Path::to_str) else { return };
    if rel.is_empty() {
        paths.insert(RESCAN.to_owned());
        return;
    }
    // A review logged, or a deck's settings changed: without them told, a running app took
    // neither into its index until some note changed (a card graded stayed due, and was asked
    // and logged again).
    let review_log = rel.strip_prefix(".knotes/reviews/").is_some_and(|name| !name.contains('/') && name.ends_with(".tsv") && !name.starts_with('.'));
    if review_log || rel == ".knotes/decks.json" {
        paths.insert(rel.to_owned());
        return;
    }
    let (folders, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let folders = folders.strip_prefix(ATTACHMENTS_FOLDER).filter(|rest| rest.is_empty() || rest.starts_with('/')).unwrap_or(folders);
    let hidden_folder = folders.split('/').any(|part| part.starts_with('.'));
    if !rel.is_empty() && !hidden_folder && (!name.starts_with('.') || name == ".knotes.json" || name == ".knotes") {
        paths.insert(rel.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(rel: &str) -> Vec<String> {
        let root = Path::new("/vault");
        let mut paths = BTreeSet::new();
        add_visible(root, &if rel.is_empty() { root.to_path_buf() } else { root.join(rel) }, &mut paths);
        paths.into_iter().collect()
    }

    #[test]
    fn the_root_stands_for_a_rescan_and_hidden_paths_go() {
        assert_eq!(kept(""), [RESCAN]);
        assert_eq!(kept("a.md"), ["a.md"]);
        assert_eq!(kept("Work/.knotes.json"), ["Work/.knotes.json"]);
        assert_eq!(kept(".knotes/attachments/p.png"), [".knotes/attachments/p.png"]);
        // The cards' files are told of: the index takes them in at the sync that follows.
        assert_eq!(kept(".knotes/reviews/mac-a1b2c3.tsv"), [".knotes/reviews/mac-a1b2c3.tsv"]);
        assert_eq!(kept(".knotes/decks.json"), [".knotes/decks.json"]);
        assert!(kept(".knotes/reviews/.mac.tsv.tmp").is_empty() && kept(".knotes/reviews/old/mac.tsv").is_empty());
        assert!(kept(".knotes/trash/1/entry.json").is_empty() && kept(".knotes/.decks.json.knotes-tmp").is_empty());
        assert!(kept(".git/HEAD").is_empty());
        assert!(kept(".DS_Store").is_empty());
    }
}
