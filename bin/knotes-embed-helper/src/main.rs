//! The helper process that runs the embedding model (ADR 0028): `knotes-embed <model.gguf>`.
//! Reads a JSON line of passages from stdin and writes a JSON line of vectors to stdout (see
//! `knotes_embed::protocol`); exits when its input ends or after `IDLE` without a request, so
//! the model's memory goes back. A model that will not load is said in each answer.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use knotes_embed::{Embedder, protocol};
use knotes_llama::LlamaEmbedder;

/// How long it waits for a request before it exits (`KNOTES_EMBED_IDLE_MS` sets it for tests).
const IDLE: Duration = Duration::from_secs(5 * 60);

fn main() {
    let Some(model) = std::env::args_os().nth(1) else {
        eprintln!("usage: knotes-embed <model.gguf>");
        std::process::exit(2);
    };
    let idle = std::env::var("KNOTES_EMBED_IDLE_MS").ok().and_then(|ms| ms.parse().ok()).map_or(IDLE, Duration::from_millis);
    let embedder = LlamaEmbedder::load(Path::new(&model));
    // Lines are read on a thread of their own, so waiting for one can time out.
    let (lines, requests) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            if line.is_err() || lines.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    let mut out = std::io::stdout().lock();
    while let Ok(line) = requests.recv_timeout(idle) {
        let result = match &embedder {
            Ok(embedder) => protocol::read_request(&line).and_then(|passages| embedder.embed(&passages)),
            Err(e) => Err(e.clone()),
        };
        if writeln!(out, "{}", protocol::response(&result)).and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    // The model is freed before the process exits (Metal checks at exit).
    drop(embedder);
}
