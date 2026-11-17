use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use knotes_core::BoxError;
use knotes_index::{SqliteIndex, is_corrupt};
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_vault::{FsVault, FsWatcher};

use crate::memory::{self, LARGE_SYNC};
use crate::{Snapshot, VaultLock, index_dir};

/// Changes within this window are synced together.
const DEBOUNCE: Duration = Duration::from_millis(200);
/// The expired among the recently deleted are looked for when the vault opens, when its root's
/// rules change (its days may have), and after a change at most this often (they expire with
/// time, not with changes).
const PURGE_EVERY: Duration = Duration::from_secs(3600);
/// Index maintenance runs this long after the last change; otherwise the thread sleeps until
/// a message arrives (no polling: an idle app does not wake up).
const MAINTENANCE_DELAY: Duration = Duration::from_secs(2);
/// After a failed sync the next try comes with the next change, or this long after.
const RETRY: Duration = Duration::from_secs(30);

/// Messages to the indexing thread.
enum Control {
    Changed(Vec<String>),
    Stop,
}

pub enum Event {
    Snapshot(Snapshot),
    /// A sync, a snapshot or index upkeep failed; the window shows the last snapshot and says
    /// it. Indexing goes on: the next change, or a retry, tries again.
    Error(String),
    /// The vault cannot be watched (why): it is indexed, but changes made outside Knotes are
    /// seen only when it is opened again.
    NotWatched(String),
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
                loop {
                    match run(&vault, &dir, &tx, changes.clone(), &control_rx) {
                        Ok(()) => return,
                        // Found damaged while open (its connection now closed): it is a cache,
                        // built again from the vault, and the session goes on.
                        Err(e) if is_corrupt(&e) => {
                            eprintln!("knotes: the index of {} is damaged, built again: {e}", vault.display());
                            if let Err(e) = SqliteIndex::recreate(&dir) {
                                let _ = tx.send(Event::Error(e.to_string()));
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(e.to_string()));
                            return;
                        }
                    }
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

/// Indexes the vault until told to stop. A failure is told (`Event::Error`) and indexing goes
/// on, tried again with the next change or after `RETRY`; only an index that cannot be opened,
/// or one found damaged (built again by the caller), ends it.
fn run(vault: &Path, dir: &Path, tx: &Sender<Event>, changes: Sender<Control>, control: &Receiver<Control>) -> Result<(), BoxError> {
    let mut index = SqliteIndex::open(dir)?;
    let files = FsVault::new(vault);
    let last: std::cell::RefCell<Option<Snapshot>> = std::cell::RefCell::new(None);
    let send = |index: &SqliteIndex, changed: Vec<String>| -> Result<bool, BoxError> {
        let snapshot = Snapshot::read(index, &files, changed, last.borrow().as_ref())?;
        *last.borrow_mut() = Some(snapshot.clone());
        Ok(tx.send(Event::Snapshot(snapshot)).is_ok())
    };
    // What a step gave: go on (`Ok(true)`), the window is gone (`Ok(false)`), or a damaged index
    // (`Err`, for the caller to build it again). Any other failure is told; `failed` says so.
    let mut failed = false;
    let settle = |result: Result<bool, BoxError>, failed: &mut bool| -> Result<bool, BoxError> {
        match result {
            Ok(going) => {
                *failed = false;
                Ok(going)
            }
            Err(e) if is_corrupt(&e) => Err(e),
            Err(e) => {
                *failed = true;
                Ok(tx.send(Event::Error(e.to_string())).is_ok())
            }
        }
    };
    purge_expired(&files);
    let mut purged_at = std::time::Instant::now();
    if !settle(send(&index, Vec::new()), &mut failed)? {
        return Ok(());
    }
    // Watch before syncing, so changes made during the sync are picked up afterwards. Without
    // the watcher (a volume FSEvents does not watch) the vault is still indexed; changes made
    // outside Knotes are then seen when it is opened again.
    let _watcher = match FsWatcher::with_callback(vault, DEBOUNCE, move |batch| changes.send(Control::Changed(batch)).is_ok()) {
        Ok(watcher) => Some(watcher),
        Err(e) => {
            if tx.send(Event::NotWatched(e.to_string())).is_err() {
                return Ok(());
            }
            None
        }
    };
    // PDFs' text is indexed too (ADR 0022).
    let indexer = Indexer::with_documents(FsVault::new(vault), MarkdownParser::new(), knotes_pdf::PdfTexts::new(vault));
    // A sync and, when it changed something or `always`, its snapshot.
    let sync = |index: &mut SqliteIndex, changed: Vec<String>, always: bool| -> Result<bool, BoxError> {
        let report = indexer.sync(index)?;
        // The reviews logged since (ADR 0037): this Mac's, and another's come with the vault.
        let reviewed = take_reviews(index, &files);
        let touched = report.added + report.modified + report.removed + reviewed;
        if touched >= LARGE_SYNC {
            memory::give_back(index);
        }
        if touched > 0 || always { send(index, changed) } else { Ok(true) }
    };
    if !settle(sync(&mut index, Vec::new(), false), &mut failed)? {
        return Ok(());
    }
    let mut maintenance_due = true;
    loop {
        let wait = if failed { Some(RETRY) } else { maintenance_due.then_some(MAINTENANCE_DELAY) };
        let message = match wait {
            Some(wait) => match control.recv_timeout(wait) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            },
            None => Some(control.recv().map_err(|_| "control channel closed")?),
        };
        match message {
            Some(Control::Changed(changed)) => {
                if purged_at.elapsed() >= PURGE_EVERY || changed.iter().any(|path| path == ".knotes.json" || path == ".knotes") {
                    purge_expired(&files);
                    purged_at = std::time::Instant::now();
                }
                if !settle(sync(&mut index, changed, true), &mut failed)? {
                    return Ok(());
                }
                maintenance_due = true;
            }
            Some(Control::Stop) => return Ok(()),
            // The retry after a failure: the whole vault looked at again.
            None if failed => {
                if !settle(sync(&mut index, Vec::new(), true), &mut failed)? {
                    return Ok(());
                }
            }
            None => match index.maintenance() {
                Ok(report) => {
                    maintenance_due = report.merge_steps > 0;
                    if report.merge_steps == 0 {
                        // Merging is done (or was not needed): whatever it allocated is free now.
                        memory::give_back(&index);
                    }
                }
                Err(e) => {
                    maintenance_due = false;
                    if !settle(Err(e), &mut failed)? {
                        return Ok(());
                    }
                }
            },
        }
    }
}

/// Takes into the index what the vault's review logs hold past where each was read to, and the
/// decks' settings when they changed (the cards are scheduled again); how many reviews were
/// new (one more for settings changed, so that the window hears of them). A log that cannot be
/// read is left for the next time, and said.
fn take_reviews(index: &mut SqliteIndex, files: &FsVault) -> usize {
    let fsrs = knotes_cards::Fsrs::default();
    let mut new = 0;
    // Settings that cannot be read now: the cards stay as scheduled, new reviews by the defaults.
    let settings = match files.read_deck_settings() {
        Ok(text) => {
            match index.take_deck_settings(&text, &fsrs) {
                Ok(changed) => new += usize::from(changed),
                Err(e) => eprintln!("knotes: cannot take the decks' settings: {e}"),
            }
            knotes_cards::DeckSettings::parse(&text)
        }
        Err(e) => {
            eprintln!("knotes: cannot read the decks' settings: {e}");
            knotes_cards::DeckSettings::default()
        }
    };
    for (name, size) in files.review_logs().unwrap_or_default() {
        let read = index.reviews_read(&name).unwrap_or(0);
        // Shorter than what was read: replaced (restored from a backup); read again whole.
        let from = if size < read { 0 } else { read };
        if size == from {
            continue;
        }
        match files.read_review_log(&name, from).map_err(BoxError::from).and_then(|text| index.take_reviews(&name, from, &text, &fsrs, &settings)) {
            Ok(taken) => new += taken,
            Err(e) => eprintln!("knotes: cannot read the review log {name}: {e}"),
        }
    }
    new
}

/// Deletes for good what has been in the trash longer than the vault's days (ADR 0032).
fn purge_expired(files: &FsVault) {
    use knotes_core::{FolderRulesStore, TrashStore};
    let root = files.read_rules(&[""]).ok().and_then(|r| r.into_iter().next()).and_then(|(_, text)| knotes_app::FolderRules::parse(&text).ok());
    let days = root.as_ref().map_or(knotes_app::DEFAULT_TRASH_DAYS, knotes_app::FolderRules::trash_days);
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let before = now_ms - i64::from(days) * 86_400_000;
    for entry in files.trashed().unwrap_or_default().into_iter().filter(|e| e.deleted_ms < before) {
        if let Err(e) = files.purge(&entry.id) {
            eprintln!("knotes: cannot delete {} for good: {e}", entry.path);
        }
    }
}
