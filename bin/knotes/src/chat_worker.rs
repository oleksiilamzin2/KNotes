//! The chat's requests off the main thread (ADR 0027): a thread of its own runs one turn at a
//! time, with the vault's tools over a reader of the index of its own. Search by meaning (ADR
//! 0028): one helper process runs the embedding model for the tools' queries and for a thread
//! that keeps the vault's vectors up to its index.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use knotes_chat::{Chat, ChatError, ChatEvent, ChatGptAccount, CredentialStore, HttpRequest, HttpResponse, Memory, Transport};
use knotes_embed::{EmbedError, Embedder, Passage, ProcessEmbedder, Vector};
use knotes_index::SqliteIndex;
use knotes_pdf::PdfTexts;
use knotes_tools::VaultTools;
use knotes_vault::FsVault;
use knotes_vectors::{EMBEDDING, MeaningIndex, VectorStore, update};
use serde_json::{Value, json};

/// Called with the conversation's items and memory after a turn, and how it went.
pub type TurnDone = Box<dyn FnOnce(Vec<Value>, Memory, Result<(), ChatError>) + Send>;

/// How often the vectors are brought up to the index.
const VECTORS_EVERY: Duration = Duration::from_secs(15);
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
    Turn(Box<Turn>),
}

pub struct ChatWorker {
    jobs: mpsc::Sender<Job>,
}

impl ChatWorker {
    pub fn new(account: Arc<Account>) -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        // None in the probe (its windows never start the model) or without the helper and model.
        let embedder: Option<Arc<ProcessEmbedder>> = if crate::probe::enabled() {
            None
        } else {
            std::env::current_exe().ok().and_then(|exe| knotes_session::embedder_files(&exe)).map(|(helper, model)| Arc::new(ProcessEmbedder::new(helper, model)))
        };
        std::thread::Builder::new()
            .name("knotes-chat".into())
            .spawn(move || {
                let mut vault: Option<(PathBuf, PathBuf)> = None;
                let mut tools: Option<VaultTools<SqliteIndex, FsVault, PdfTexts>> = None;
                let mut vectors_stop = Arc::new(AtomicBool::new(false));
                for job in receiver {
                    match job {
                        Job::Vault(root, index_dir) => {
                            vectors_stop.store(true, Ordering::Relaxed);
                            vectors_stop = Arc::new(AtomicBool::new(false));
                            if let Some(embedder) = &embedder {
                                keep_vectors(index_dir.clone(), Arc::clone(embedder), Arc::clone(&vectors_stop));
                            }
                            vault = Some((root, index_dir));
                            tools = None;
                        }
                        Job::Turn(mut turn) => {
                            if tools.is_none()
                                && let Some((root, dir)) = &vault
                            {
                                tools = SqliteIndex::open_reader(dir).ok().map(|index| {
                                    let tools = VaultTools::new(index, FsVault::new(root), PdfTexts::new(root));
                                    match (&embedder, VectorStore::open_reader(dir)) {
                                        (Some(embedder), Ok(store)) => tools.with_meaning(Box::new(MeaningIndex::over_store(Arc::clone(embedder) as Arc<dyn Embedder>, store))),
                                        _ => tools,
                                    }
                                });
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
        Self { jobs }
    }

    pub fn set_vault(&self, root: PathBuf, index_dir: PathBuf) {
        let _ = self.jobs.send(Job::Vault(root, index_dir));
    }

    pub fn send(&self, turn: Turn) {
        let _ = self.jobs.send(Job::Turn(Box::new(turn)));
    }
}

/// Keeps the vectors of the vault indexed in `dir` up to its index, on a thread of its own: at
/// once, then every `VECTORS_EVERY`, until `stop` (another vault opened).
fn keep_vectors(dir: PathBuf, embedder: Arc<ProcessEmbedder>, stop: Arc<AtomicBool>) {
    let spawned = std::thread::Builder::new().name("knotes-vectors".into()).spawn(move || {
        let paced = Paced(embedder);
        let mut told = false;
        while !stop.load(Ordering::Relaxed) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64);
            let result = SqliteIndex::open_reader(&dir).and_then(|index| update(&index, &VectorStore::open(&dir, EMBEDDING)?, &paced, now, &stop));
            if let Err(e) = result
                && !told
            {
                eprintln!("knotes: search by meaning: {e}");
                told = true;
            }
            for _ in 0..VECTORS_EVERY.as_secs() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("knotes: cannot start keeping the vectors: {e}");
    }
}

/// The embedder, pausing after each batch.
struct Paced(Arc<ProcessEmbedder>);

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
