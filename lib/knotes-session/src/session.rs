use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use knotes_core::BoxError;
use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_vault::{FsVault, FsWatcher};

use crate::memory::{self, LARGE_SYNC};
use crate::{Snapshot, VaultLock, index_dir};

/// Changes within this window are synced together.
const DEBOUNCE: Duration = Duration::from_millis(200);
/// Index maintenance runs this long after the last change; otherwise the thread sleeps until
/// a message arrives (no polling: an idle app does not wake up).
const MAINTENANCE_DELAY: Duration = Duration::from_secs(2);

/// Messages to the indexing thread.
enum Control {
    Changed(Vec<String>),
    Stop,
}

pub enum Event {
    Snapshot(Snapshot),
    /// Indexing stopped; the window shows the last snapshot and the error.
    Error(String),
}

/// An open vault. Dropping it stops the background thread, then releases the lock.
pub struct VaultSession {
    dir: PathBuf,
    control: Sender<Control>,
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
        let (control, control_rx) = mpsc::channel();
        let worker = {
            let (vault, dir, changes) = (vault.to_path_buf(), dir.clone(), control.clone());
            thread::Builder::new().name("knotes-index".into()).spawn(move || {
                if let Err(e) = run(&vault, &dir, &tx, changes, &control_rx) {
                    let _ = tx.send(Event::Error(e.to_string()));
                }
            })?
        };
        Ok((Self { dir, control, worker: Some(worker), _lock: lock }, rx))
    }

    /// Folder of the vault's index, for read-only connections (`SqliteIndex::open_reader`).
    pub fn index_dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(vault: &Path, dir: &Path, tx: &Sender<Event>, changes: Sender<Control>, control: &Receiver<Control>) -> Result<(), BoxError> {
    let mut index = SqliteIndex::open(dir)?;
    let files = FsVault::new(vault);
    let last: std::cell::RefCell<Option<Snapshot>> = std::cell::RefCell::new(None);
    let send = |index: &SqliteIndex, changed: Vec<String>| -> Result<bool, BoxError> {
        let snapshot = Snapshot::read(index, &files, changed, last.borrow().as_ref())?;
        *last.borrow_mut() = Some(snapshot.clone());
        Ok(tx.send(Event::Snapshot(snapshot)).is_ok())
    };
    send(&index, Vec::new())?;
    // Watch before syncing, so changes made during the sync are picked up afterwards.
    let _watcher = FsWatcher::with_callback(vault, DEBOUNCE, move |batch| changes.send(Control::Changed(batch)).is_ok())?;
    // PDFs' text is indexed too (ADR 0022).
    let indexer = Indexer::with_documents(FsVault::new(vault), MarkdownParser::new(), knotes_pdf::PdfTexts::new(vault));
    let first = indexer.sync(&mut index)?;
    let touched = first.added + first.modified + first.removed;
    if touched >= LARGE_SYNC {
        memory::give_back(&index);
    }
    if touched > 0 && !send(&index, Vec::new())? {
        return Ok(());
    }
    let mut maintenance_due = true;
    loop {
        let message = if maintenance_due {
            match control.recv_timeout(MAINTENANCE_DELAY) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        } else {
            Some(control.recv().map_err(|_| "control channel closed")?)
        };
        match message {
            Some(Control::Changed(changed)) => {
                let report = indexer.sync(&mut index)?;
                if report.added + report.modified + report.removed >= LARGE_SYNC {
                    memory::give_back(&index);
                }
                if !send(&index, changed)? {
                    return Ok(());
                }
                maintenance_due = true;
            }
            Some(Control::Stop) => return Ok(()),
            None => {
                let merged = index.maintenance()?.merge_steps;
                maintenance_due = merged > 0;
                if merged == 0 {
                    // Merging is done (or was not needed): whatever it allocated is free now.
                    memory::give_back(&index);
                }
            }
        }
    }
}
