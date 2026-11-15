use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use crate::{EmbedError, Embedder, Passage, Vector, protocol};

/// The embedder in a helper process (`knotes-embed <model>`, ADR 0028): started at the first
/// request, and again when it has exited (it does after a few idle minutes) or died.
/// Its stderr goes nowhere: llama.cpp's Metal logs cannot be silenced, and errors come back
/// in the protocol.
pub struct ProcessEmbedder {
    helper: PathBuf,
    model: PathBuf,
    running: Mutex<Option<Running>>,
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ProcessEmbedder {
    pub fn new(helper: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        Self { helper: helper.into(), model: model.into(), running: Mutex::new(None) }
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
        match (stdin, stdout) {
            (Some(stdin), Some(stdout)) => Ok(Running { child, stdin, stdout: BufReader::new(stdout) }),
            _ => Err(EmbedError::Unavailable("the helper's pipes".into())),
        }
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
    /// The helper's answer to `line`; none when it has gone.
    fn ask(&mut self, line: &str) -> Option<String> {
        writeln!(self.stdin, "{line}").and_then(|()| self.stdin.flush()).ok()?;
        let mut reply = String::new();
        (self.stdout.read_line(&mut reply).ok()? > 0).then_some(reply)
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
        for _ in 0..2 {
            let helper = match running.as_mut() {
                Some(helper) => helper,
                None => running.insert(self.start()?),
            };
            if let Some(reply) = helper.ask(&line) {
                let vectors = protocol::read_response(&reply)?;
                if vectors.len() != passages.len() {
                    return Err(EmbedError::Failed(format!("{} vectors for {} passages", vectors.len(), passages.len())));
                }
                return Ok(vectors);
            }
            if let Some(gone) = running.take() {
                gone.stop();
            }
        }
        Err(EmbedError::Failed("the helper stopped answering".into()))
    }
}

impl Drop for ProcessEmbedder {
    fn drop(&mut self) {
        if let Some(running) = self.running.get_mut().ok().and_then(Option::take) {
            running.stop();
        }
    }
}
