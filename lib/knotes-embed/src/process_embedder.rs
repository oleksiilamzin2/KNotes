use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use crate::{EmbedError, Embedder, Passage, Vector, protocol};

/// How long an answer may take, the model's loading included (3 s the first time on an M5 Pro,
/// a batch 80 ms): a helper silent longer has hung, and is stopped. Everything asking waited
/// for it for good: the list's searches (all of them, on one thread), the chat's turn, the
/// vectors kept.
pub const DEADLINE: Duration = Duration::from_secs(60);

/// The embedder in a helper process (`knotes-embed <model>`, ADR 0028): started at the first
/// request, and again when it has exited (it does after a few idle minutes), died or hung.
/// Its stderr goes nowhere: llama.cpp's Metal logs cannot be silenced, and errors come back
/// in the protocol.
pub struct ProcessEmbedder {
    helper: PathBuf,
    model: PathBuf,
    deadline: Duration,
    running: Mutex<Option<Running>>,
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    /// Its answers, a line each, read on a thread of their own: waiting for one can time out.
    replies: Receiver<String>,
}

/// Why no answer came.
enum NoAnswer {
    /// The helper has gone (exited, or its pipes closed).
    Gone,
    /// Nothing within the deadline.
    Late,
}

impl ProcessEmbedder {
    pub fn new(helper: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        Self::with_deadline(helper, model, DEADLINE)
    }

    /// One whose helper is stopped when an answer takes longer than `deadline` (`DEADLINE`).
    pub fn with_deadline(helper: impl Into<PathBuf>, model: impl Into<PathBuf>, deadline: Duration) -> Self {
        Self { helper: helper.into(), model: model.into(), deadline, running: Mutex::new(None) }
    }

    fn start(&self) -> Result<Running, EmbedError> {
        let mut child = Command::new(&self.helper)
            .arg(&self.model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| EmbedError::Unavailable(format!("{}: {e}", self.helper.display())))?;
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
        let (Some(stdin), Some(stdout)) = (stdin, stdout) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EmbedError::Unavailable("the helper's pipes".into()));
        };
        let (lines, replies) = mpsc::channel();
        // Ends with the helper's output (it exited, or was stopped).
        let reader = std::thread::Builder::new().name("knotes-embed-replies".into()).spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.is_err() || lines.send(line.unwrap_or_default()).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = reader {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EmbedError::Unavailable(format!("reading the helper: {e}")));
        }
        Ok(Running { child, stdin, replies })
    }
}

impl ProcessEmbedder {
    /// Starts the helper ahead of a request (it loads its model as it starts), unless it is
    /// running; never waits, not even for a request under way (the helper is running then).
    pub fn warm(&self) {
        let Ok(mut running) = self.running.try_lock() else { return };
        if let Some(helper) = running.as_mut()
            && !matches!(helper.child.try_wait(), Ok(Some(_)))
        {
            return;
        }
        if let Some(gone) = running.take() {
            gone.stop();
        }
        *running = self.start().ok();
    }
}

impl Running {
    /// The helper's answer to `line`, within `deadline`.
    fn ask(&mut self, line: &str, deadline: Duration) -> Result<String, NoAnswer> {
        writeln!(self.stdin, "{line}").and_then(|()| self.stdin.flush()).map_err(|_| NoAnswer::Gone)?;
        self.replies.recv_timeout(deadline).map_err(|e| match e {
            RecvTimeoutError::Timeout => NoAnswer::Late,
            RecvTimeoutError::Disconnected => NoAnswer::Gone,
        })
    }

    /// Stops the helper; how it ended (on its own, if it had).
    fn stop(mut self) -> Option<ExitStatus> {
        let _ = self.child.kill();
        self.child.wait().ok()
    }
}

impl Embedder for ProcessEmbedder {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }
        let line = protocol::request(passages);
        let mut running = self.running.lock().map_err(|_| EmbedError::Failed("the helper's lock was poisoned".into()))?;
        // Once as it is, once more after starting it again if it had gone.
        let mut ended = None;
        for _ in 0..2 {
            let helper = match running.as_mut() {
                Some(helper) => helper,
                None => running.insert(self.start()?),
            };
            match helper.ask(&line, self.deadline) {
                Ok(reply) => {
                    let vectors = protocol::read_response(&reply)?;
                    if vectors.len() != passages.len() {
                        return Err(EmbedError::Failed(format!("{} vectors for {} passages", vectors.len(), passages.len())));
                    }
                    return Ok(vectors);
                }
                // Hung: stopped (an answer coming late would be taken for the next request's),
                // and not asked again, which would make the asking wait as long once more.
                Err(NoAnswer::Late) => {
                    if let Some(hung) = running.take() {
                        hung.stop();
                    }
                    return Err(EmbedError::Failed(format!("the helper did not answer within {} s", self.deadline.as_secs_f64())));
                }
                Err(NoAnswer::Gone) => ended = running.take().and_then(Running::stop),
            }
        }
        Err(EmbedError::Failed(match ended {
            Some(status) => format!("the helper stopped answering ({status})"),
            None => "the helper stopped answering".into(),
        }))
    }
}

impl Drop for ProcessEmbedder {
    fn drop(&mut self) {
        if let Some(running) = self.running.get_mut().ok().and_then(Option::take) {
            running.stop();
        }
    }
}
