//! The chat's requests off the main thread (ADR 0027): a thread of its own runs one turn at a
//! time, with the vault's tools over a reader of the index of its own. Search by meaning (ADR
//! 0028): one helper process runs the embedding model for the tools' queries and for a thread
//! that keeps the vault's vectors up to its index.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use knotes_chat::{Chat, ChatError, ChatEvent, ChatGptAccount, CredentialStore, HttpRequest, HttpResponse, Memory, Transport};
use knotes_embed::{EmbedError, Embedder, Passage, Vector};
use knotes_index::SqliteIndex;
use knotes_pdf::PdfTexts;
use knotes_tools::VaultTools;
use knotes_vault::FsVault;
use knotes_vectors::{MeaningIndex, MeaningModel, Updated, VectorStore, Which, update, update_reporting};
use serde_json::{Value, json};

/// Called with the conversation's items and memory after a turn, and how it went.
pub type TurnDone = Box<dyn FnOnce(Vec<Value>, Memory, Result<(), ChatError>) + Send>;

/// How long the index must stay unchanged before a pass over the vectors: changes come in runs
/// (a save every two seconds while the user types), one pass for the run. A document is
/// embedded once unchanged for `knotes_vectors::SETTLE` (15 s; a minute under active editing),
/// or at once before the chat asks.
const VECTORS_QUIET: Duration = Duration::from_secs(2);
/// A pause after each batch embedded in the background, leaving the GPU to the window.
const VECTORS_PAUSE: Duration = Duration::from_millis(20);

/// Waits before sending again while OpenAI is unavailable or the network fails.
const RETRY_AFTER: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];

pub type Account = ChatGptAccount<Box<dyn Transport + Send + Sync>, Box<dyn CredentialStore + Send + Sync>>;

/// One turn to run: the conversation so far and the question, and where to say what happens.
pub struct Turn {
    pub items: Vec<Value>,
    pub memory: Memory,
    /// The size (tokens) past which the conversation's older part is summarized.
    pub compact_at: u64,
    pub text: String,
    pub open: Option<String>,
    pub model: String,
    /// How hard the model reasons; none, its own default.
    pub effort: Option<String>,
    pub cache_key: String,
    pub cancel: Arc<AtomicBool>,
    pub on_event: Box<dyn FnMut(ChatEvent) + Send>,
    /// The conversation's items after the turn (as they were when it failed).
    pub done: TurnDone,
}

enum Job {
    Vault(PathBuf, PathBuf),
    /// Another model chosen for search by meaning (or none).
    Meaning,
    Turn(Box<Turn>),
}

pub struct ChatWorker {
    jobs: mpsc::Sender<Job>,
    /// Wakes the thread keeping the open vault's vectors (none without the embedding model).
    vectors_wake: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl ChatWorker {
    pub fn new(account: Arc<Account>) -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        let vectors_wake: Arc<Mutex<Option<mpsc::Sender<()>>>> = Arc::default();
        let wake = Arc::clone(&vectors_wake);
        std::thread::Builder::new()
            .name("knotes-chat".into())
            .spawn(move || {
                let mut vault: Option<(PathBuf, PathBuf)> = None;
                let mut tools: Option<VaultTools<SqliteIndex, FsVault, PdfTexts>> = None;
                let mut vectors_stop = Arc::new(AtomicBool::new(false));
                // The thread keeping the vectors of `dir` with the model chosen, the last one's
                // ended (its sender dropped).
                let restart_vectors = |dir: &PathBuf, stop: &mut Arc<AtomicBool>| {
                    stop.store(true, Ordering::Relaxed);
                    *stop = Arc::new(AtomicBool::new(false));
                    let mut wake = wake.lock().unwrap_or_else(|e| e.into_inner());
                    *wake = None;
                    if let (Some(model), Some(embedder)) = (crate::meaning::model(), crate::meaning::embedder()) {
                        let (sender, receiver) = mpsc::channel();
                        *wake = Some(sender);
                        keep_vectors(dir.clone(), embedder as Arc<dyn Embedder>, model, receiver, VECTORS_QUIET, Arc::clone(stop));
                    }
                };
                for job in receiver {
                    match job {
                        Job::Vault(root, index_dir) => {
                            restart_vectors(&index_dir, &mut vectors_stop);
                            vault = Some((root, index_dir));
                            tools = None;
                        }
                        Job::Meaning => {
                            if let Some((_, dir)) = &vault {
                                restart_vectors(dir, &mut vectors_stop);
                            }
                            tools = None;
                        }
                        Job::Turn(mut turn) => {
                            let meaning = crate::meaning::model().zip(crate::meaning::embedder());
                            if tools.is_none()
                                && let Some((root, dir)) = &vault
                            {
                                tools = SqliteIndex::open_reader(dir).ok().map(|index| {
                                    let tools = VaultTools::new(index, FsVault::new(root), PdfTexts::new(root));
                                    match meaning.as_ref().and_then(|(model, embedder)| VectorStore::open_reader(dir, &model.store_file()).ok().map(|store| (embedder, store))) {
                                        Some((embedder, store)) => tools.with_meaning(Box::new(MeaningIndex::over_store(Arc::clone(embedder) as Arc<dyn Embedder>, store))),
                                        None => tools,
                                    }
                                });
                            }
                            // What the user has just written, embedded before the model searches.
                            if let (Some((model, embedder)), Some((_, dir))) = (&meaning, &vault)
                                && let Err(e) = embed_waiting(dir, &**embedder, *model)
                            {
                                eprintln!("knotes: search by meaning: {e}");
                            }
                            let mut items = std::mem::take(&mut turn.items);
                            let mut memory = std::mem::take(&mut turn.memory);
                            let result = match &tools {
                                None => Err(ChatError::Failed("the vault's index is not ready yet".into())),
                                Some(tools) => {
                                    let chat = Chat { transport: account.transport(), token: &*account, model: turn.model.clone(), cache_key: turn.cache_key.clone(), retry_after: RETRY_AFTER.to_vec(), compact_at: turn.compact_at, today: local_today(), effort: turn.effort.clone() };
                                    chat.send(&mut items, &mut memory, &turn.text, turn.open.as_deref(), tools, &turn.cancel, &mut *turn.on_event)
                                }
                            };
                            (turn.done)(items, memory, result);
                        }
                    }
                }
            })
            .expect("the chat's thread starts");
        Self { jobs, vectors_wake }
    }

    /// The user is writing a question: the embedding model is loaded meanwhile, so the turn's
    /// first search by meaning does not wait for it (ADR 0028). Never waits itself.
    pub fn warm_up(&self) {
        if let Some(embedder) = crate::meaning::embedder() {
            embedder.warm();
        }
    }

    /// Another model was chosen for search by meaning (or none): the vectors are kept with it
    /// from now on, and the next turn's tools search with it.
    pub fn meaning_changed(&self) {
        let _ = self.jobs.send(Job::Meaning);
    }

    /// The vault's index has changed: its vectors are to be brought up to it.
    pub fn index_changed(&self) {
        if let Some(wake) = self.vectors_wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = wake.send(());
        }
    }

    pub fn set_vault(&self, root: PathBuf, index_dir: PathBuf) {
        let _ = self.jobs.send(Job::Vault(root, index_dir));
    }

    pub fn send(&self, turn: Turn) {
        let _ = self.jobs.send(Job::Turn(Box::new(turn)));
    }
}

/// Keeps the vectors of the vault indexed in `dir` up to its index, on a thread of its own: at
/// once, then each time it is woken (the index changed) and the index has stayed unchanged for
/// `quiet`, or when a document left for being written settles. Asleep otherwise: nothing is
/// looked at while nothing changes. Ends when its waker is dropped (another vault opened).
fn keep_vectors(dir: PathBuf, embedder: Arc<dyn Embedder>, model: MeaningModel, wake: mpsc::Receiver<()>, quiet: Duration, stop: Arc<AtomicBool>) -> Option<std::thread::JoinHandle<()>> {
    let spawned = std::thread::Builder::new().name("knotes-vectors".into()).spawn(move || {
        let paced = Paced(embedder);
        let mut told = false;
        loop {
            // How far each pass goes, for the settings' progress bar.
            let pass = SqliteIndex::open_reader(&dir).and_then(|index| update_reporting(&index, &store(&dir, model)?, &paced, now_ns(), Which::Settled, &stop, &mut |p| crate::meaning::report(&dir, p)));
            crate::meaning::finished(&dir);
            let next = match pass {
                Ok(updated) => updated.next,
                Err(e) => {
                    if !told {
                        eprintln!("knotes: search by meaning: {e}");
                        told = true;
                    }
                    None
                }
            };
            let woke = match next {
                Some(wait) => !matches!(wake.recv_timeout(wait), Err(mpsc::RecvTimeoutError::Disconnected)),
                None => wake.recv().is_ok(),
            };
            // Then a quiet moment, the run of changes over.
            while woke && wake.recv_timeout(quiet).is_ok() {}
            if !woke || stop.load(Ordering::Relaxed) {
                return;
            }
        }
    });
    spawned.map_err(|e| eprintln!("knotes: cannot start keeping the vectors: {e}")).ok()
}

/// Embeds at once the documents of the vault indexed in `dir` changed too lately for the
/// background (still settling), so the turn's search finds what the user has just written.
fn embed_waiting(dir: &Path, embedder: &dyn Embedder, model: MeaningModel) -> Result<Updated, knotes_core::BoxError> {
    let index = SqliteIndex::open_reader(dir)?;
    update(&index, &store(dir, model)?, embedder, now_ns(), Which::Waiting, &AtomicBool::new(false))
}

/// The store of `model`'s vectors for the vault indexed in `dir`.
fn store(dir: &Path, model: MeaningModel) -> Result<VectorStore, knotes_core::BoxError> {
    VectorStore::open(dir, &model.store_file(), model.embedding())
}

fn now_ns() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64)
}

/// The embedder, pausing after each batch.
struct Paced(Arc<dyn Embedder>);

impl Embedder for Paced {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        let vectors = self.0.embed(passages);
        std::thread::sleep(VECTORS_PAUSE);
        vectors
    }
}

/// The local day for the model, from the system's time zone: "Saturday, 2026-11-14 (UTC+02:00)".
pub fn local_today() -> String {
    let offset = i64::try_from(objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT()).unwrap_or(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + offset;
    // 1 January 1970 was a Thursday.
    let weekday = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"][now.div_euclid(86_400).rem_euclid(7) as usize];
    let sign = if offset < 0 { '-' } else { '+' };
    format!("{weekday}, {} (UTC{sign}{:02}:{:02})", &knotes_tools::utc_minute(now)[..10], offset.abs() / 3_600, offset.abs() % 3_600 / 60)
}

/// Probe: OpenAI as answers read from a file (`KNOTES_PROBE_CHAT`), a JSON list of
/// `{"status", "body"}` taken in turn; the models list answers by itself.
pub struct ScriptTransport(Mutex<VecDeque<(u16, String)>>);

impl ScriptTransport {
    pub fn from_file(path: &str) -> Self {
        let value: Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let answers = value.as_array().map(|list| list.iter().map(|a| (a["status"].as_u64().unwrap_or(200) as u16, a["body"].as_str().unwrap_or_default().to_string())).collect()).unwrap_or_default();
        Self(Mutex::new(answers))
    }
}

impl Transport for ScriptTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, ChatError> {
        let (status, body) = if request.url.ends_with("/models") {
            (200, json!({"models": [{"slug": "probe-model", "display_name": "Probe Model", "visibility": "list"}]}).to_string())
        } else {
            self.0.lock().unwrap().pop_front().ok_or_else(|| ChatError::Network("no answer left".into()))?
        };
        Ok(HttpResponse { status, body: Box::new(std::io::Cursor::new(body.into_bytes())) })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, VaultFile};
    use knotes_embed::{DIMS, EmbedError, Embedder, Passage, Vector};
    use knotes_index::SqliteIndex;

    /// Counts the passages it is given.
    #[derive(Default)]
    struct Counting(AtomicUsize);

    impl Embedder for Counting {
        fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
            self.0.fetch_add(passages.len(), Ordering::Relaxed);
            Ok(vec![[1; DIMS]; passages.len()])
        }
    }

    fn add(index: &mut SqliteIndex, path: &str, text: &str) {
        add_at(index, path, text, 1);
    }

    fn add_at(index: &mut SqliteIndex, path: &str, text: &str, mtime_ns: i64) {
        let parsed = knotes_md::MarkdownParser::new().parse(text);
        let file = VaultFile { path: path.into(), kind: FileKind::Markdown, size: 1, mtime_ns };
        index.apply(&[IndexChange::Upsert { file: &file, note: Some((text, &parsed)) }]).unwrap();
    }

    fn wait_for(done: impl Fn() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn before_a_turn_what_was_just_written_is_embedded_and_the_backlog_left() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        add(&mut index, "Backlog.md", "Long settled, never embedded.");
        add_at(&mut index, "Typing.md", "Just written.", super::now_ns());
        let embedder = Counting::default();
        let updated = super::embed_waiting(dir.path(), &embedder, knotes_vectors::MeaningModel::DEFAULT).unwrap();
        assert_eq!((updated.documents, embedder.0.load(Ordering::Relaxed)), (1, 1));
        let store = knotes_vectors::VectorStore::open_reader(dir.path(), &knotes_vectors::MeaningModel::DEFAULT.store_file()).unwrap();
        assert_eq!(store.dates().unwrap().keys().collect::<Vec<_>>(), ["Typing.md"]);
    }

    #[test]
    fn the_vectors_follow_the_index_when_woken_and_rest_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        add(&mut index, "a.md", "First note.");
        let embedder = Arc::new(Counting::default());
        let (wake, woken) = mpsc::channel();
        let thread = super::keep_vectors(dir.path().to_path_buf(), Arc::clone(&embedder) as Arc<dyn Embedder>, knotes_vectors::MeaningModel::DEFAULT, woken, Duration::from_millis(50), Arc::new(AtomicBool::new(false))).unwrap();
        let embedded = || embedder.0.load(Ordering::Relaxed);
        wait_for(|| embedded() == 1);
        add(&mut index, "b.md", "Second note.");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(embedded(), 1, "not woken: nothing looked at");
        wake.send(()).unwrap();
        wait_for(|| embedded() == 2);
        drop(wake);
        thread.join().unwrap();
    }

    #[test]
    fn the_local_day_names_its_weekday_date_and_offset() {
        let today = super::local_today();
        let (weekday, rest) = today.split_once(", ").unwrap();
        assert!(["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"].contains(&weekday), "{today}");
        let (date, zone) = rest.split_once(' ').unwrap();
        assert_eq!(date.len(), 10, "{today}");
        assert!(zone.starts_with("(UTC+") || zone.starts_with("(UTC-"), "{today}");
        println!("{today}");
    }
}
