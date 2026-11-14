//! The chat's requests off the main thread (ADR 0027): a thread of its own runs one turn at a
//! time, with the vault's tools over a reader of the index of its own.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use knotes_chat::{Chat, ChatError, ChatEvent, ChatGptAccount, CredentialStore, HttpRequest, HttpResponse, Summary, Transport};
use knotes_index::SqliteIndex;
use knotes_pdf::PdfTexts;
use knotes_tools::VaultTools;
use knotes_vault::FsVault;
use serde_json::{Value, json};

/// Called with the conversation's items and summary after a turn, and how it went.
pub type TurnDone = Box<dyn FnOnce(Vec<Value>, Option<Summary>, Result<(), ChatError>) + Send>;

/// Waits before sending again while OpenAI is unavailable or the network fails.
const RETRY_AFTER: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];

pub type Account = ChatGptAccount<Box<dyn Transport + Send + Sync>, Box<dyn CredentialStore + Send + Sync>>;

/// One turn to run: the conversation so far and the question, and where to say what happens.
pub struct Turn {
    pub items: Vec<Value>,
    pub summary: Option<Summary>,
    pub text: String,
    pub open: Option<String>,
    pub model: String,
    pub cache_key: String,
    pub cancel: Arc<AtomicBool>,
    pub on_event: Box<dyn FnMut(ChatEvent) + Send>,
    /// The conversation's items after the turn (as they were when it failed).
    pub done: TurnDone,
}

enum Job {
    Vault(PathBuf, PathBuf),
    Turn(Turn),
}

pub struct ChatWorker {
    jobs: mpsc::Sender<Job>,
}

impl ChatWorker {
    pub fn new(account: Arc<Account>) -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("knotes-chat".into())
            .spawn(move || {
                let mut vault: Option<(PathBuf, PathBuf)> = None;
                let mut tools: Option<VaultTools<SqliteIndex, FsVault, PdfTexts>> = None;
                for job in receiver {
                    match job {
                        Job::Vault(root, index_dir) => {
                            vault = Some((root, index_dir));
                            tools = None;
                        }
                        Job::Turn(mut turn) => {
                            if tools.is_none()
                                && let Some((root, dir)) = &vault
                            {
                                tools = SqliteIndex::open_reader(dir).ok().map(|index| VaultTools::new(index, FsVault::new(root), PdfTexts::new(root)));
                            }
                            let mut items = std::mem::take(&mut turn.items);
                            let mut summary = turn.summary.take();
                            let result = match &tools {
                                None => Err(ChatError::Failed("the vault's index is not ready yet".into())),
                                Some(tools) => {
                                    let chat = Chat { transport: account.transport(), token: &*account, model: turn.model.clone(), cache_key: turn.cache_key.clone(), retry_after: RETRY_AFTER.to_vec() };
                                    chat.send(&mut items, &mut summary, &turn.text, turn.open.as_deref(), tools, &turn.cancel, &mut *turn.on_event)
                                }
                            };
                            (turn.done)(items, summary, result);
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
        let _ = self.jobs.send(Job::Turn(turn));
    }
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
