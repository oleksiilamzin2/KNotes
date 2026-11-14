//! A run of test questions through the chat, for checking its answers by hand (ADR 0027):
//! `KNOTES_CHAT_EVAL=questions.json` (a list of `{"id", "question", "expected", "open"?}`) and
//! `KNOTES_CHAT_EVAL_OUT=answers.md`. Each question is asked in a conversation of its own, never
//! saved, with the document `open` names offered; its answer, the tools it called, its size and
//! time are written down as they come, and a last line says the run is over.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use knotes_chat::{ChatEvent, Memory};
use serde_json::Value;

use crate::chat_worker::Turn;

pub struct Question {
    id: u64,
    question: String,
    expected: String,
    open: Option<String>,
}

/// The questions in `path`, in order.
pub fn questions(path: &Path) -> Vec<Question> {
    let value: Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let text = |q: &Value, key: &str| q.get(key).and_then(Value::as_str).map(str::to_string);
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|q| Some(Question { id: q.get("id").and_then(Value::as_u64).unwrap_or(0), question: text(q, "question")?, expected: text(q, "expected").unwrap_or_default(), open: text(q, "open") }))
                .collect()
        })
        .unwrap_or_default()
}

/// Where the answers go, and how far the run is.
struct Log {
    file: File,
    total: usize,
    done: usize,
    /// When the last answer came (the next one's time is counted from it).
    last: Instant,
}

/// The turns asking `questions` with `model`, writing to `out` (begun with a heading naming
/// `model_name`).
pub fn turns(questions: Vec<Question>, model: &str, model_name: &str, compact_at: u64, out: &Path) -> std::io::Result<Vec<Turn>> {
    let mut file = File::create(out)?;
    writeln!(file, "# Перевірка ШІ: {} питань, модель {model_name}\n", questions.len())?;
    let log = Arc::new(Mutex::new(Log { file, total: questions.len(), done: 0, last: Instant::now() }));
    Ok(questions
        .into_iter()
        .map(|q| {
            let tools = Arc::new(Mutex::new(Vec::<String>::new()));
            let seen = Arc::clone(&tools);
            let log = Arc::clone(&log);
            Turn {
                items: Vec::new(),
                memory: Memory::default(),
                compact_at,
                text: q.question.clone(),
                open: q.open.clone(),
                model: model.to_string(),
                cache_key: format!("eval-{}", q.id),
                cancel: Arc::new(AtomicBool::new(false)),
                on_event: Box::new(move |event| {
                    if let ChatEvent::ToolCall { name, arguments } = event {
                        seen.lock().unwrap().push(format!("{name} {arguments}"));
                    }
                }),
                done: Box::new(move |items, memory, result| {
                    let answer = items
                        .iter()
                        .rev()
                        .find(|i| i["type"] == "message" && i["role"] == "assistant")
                        .and_then(|i| i["content"][0]["text"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    let answer = match result {
                        Ok(()) => answer,
                        Err(e) => format!("**Помилка:** {e}"),
                    };
                    let tools = tools.lock().unwrap();
                    let mut log = log.lock().unwrap();
                    let seconds = log.last.elapsed().as_secs_f64();
                    log.last = Instant::now();
                    log.done += 1;
                    let mut entry = format!("## {}. {}\n\n", q.id, q.question);
                    if let Some(open) = &q.open {
                        entry.push_str(&format!("Відкрито: {open}\n\n"));
                    }
                    entry.push_str(&format!("**Очікується:** {}\n\n", q.expected));
                    entry.push_str(&format!("**Інструменти ({}):** {}\n\n", tools.len(), if tools.is_empty() { "—".to_string() } else { tools.join("; ") }));
                    entry.push_str(&format!("**Токени:** {} · {seconds:.1} с\n\n", memory.tokens.map_or("?".to_string(), |t| t.to_string())));
                    entry.push_str(&format!("**Відповідь:**\n\n{answer}\n\n"));
                    if log.done == log.total {
                        entry.push_str(&format!("---\n\nГотово: {} з {}\n", log.done, log.total));
                    }
                    if let Err(e) = log.file.write_all(entry.as_bytes()).and_then(|()| log.file.flush()) {
                        eprintln!("knotes: cannot write the answers: {e}");
                    }
                }),
            }
        })
        .collect())
}
