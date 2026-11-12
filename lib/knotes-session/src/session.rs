use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use knotes_core::{BoxError, ChangeFeed};
use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_vault::{FsVault, FsWatcher};

use crate::{Snapshot, VaultLock, index_dir};

/// How long the background thread waits for file changes before checking for shutdown.
const POLL: Duration = Duration::from_millis(300);
/// Changes within this window are synced together.
const DEBOUNCE: Duration = Duration::from_millis(200);

pub enum Event {
    Snapshot(Snapshot),
    /// Indexing stopped; the window shows the last snapshot and the error.
    Error(String),
}

/// An open vault. Dropping it stops the background thread, then releases the lock.
pub struct VaultSession {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    _lock: VaultLock,
}

impl VaultSession {
    /// Locks the vault's index and starts indexing on a background thread. The first event is
    /// a snapshot of the existing index (instant on a warm start), the next follow the sync.
    pub fn open(vault: &Path, app_data: &Path) -> Result<(Self, Receiver<Event>), BoxError> {
        let dir = index_dir(app_data, vault);
        let lock = VaultLock::acquire(&dir)?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (vault, stop) = (vault.to_path_buf(), Arc::clone(&stop));
            thread::Builder::new().name("knotes-index".into()).spawn(move || {
                if let Err(e) = run(&vault, &dir, &tx, &stop) {
                    let _ = tx.send(Event::Error(e.to_string()));
                }
            })?
        };
        Ok((Self { stop, worker: Some(worker), _lock: lock }, rx))
    }
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(vault: &PathBuf, dir: &Path, tx: &Sender<Event>, stop: &AtomicBool) -> Result<(), BoxError> {
    let mut index = SqliteIndex::open(dir)?;
    let files = FsVault::new(vault);
    let send = |index: &SqliteIndex, changed: Vec<String>| -> Result<bool, BoxError> {
        Ok(tx.send(Event::Snapshot(Snapshot::read(index, &files, changed)?)).is_ok())
    };
    send(&index, Vec::new())?;
    // Watch before syncing, so changes made during the sync are picked up afterwards.
    let watcher = FsWatcher::new(vault, DEBOUNCE)?;
    let indexer = Indexer::new(FsVault::new(vault), MarkdownParser::new());
    let first = indexer.sync(&mut index)?;
    if first.added + first.modified + first.removed > 0 && !send(&index, Vec::new())? {
        return Ok(());
    }
    let mut maintenance_due = true;
    while !stop.load(Ordering::Relaxed) {
        match watcher.next_change(POLL) {
            Some(changed) => {
                indexer.sync(&mut index)?;
                if !send(&index, changed)? {
                    return Ok(());
                }
                maintenance_due = true;
            }
            None if maintenance_due => maintenance_due = index.maintenance()?.merge_steps > 0,
            None => {}
        }
    }
    Ok(())
}
