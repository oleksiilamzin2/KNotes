use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::authorized::{error_of, send_authorized};
use crate::context::{Memory, Summary, context, estimate_tokens, recent_start, transcript, turn_starts};
use crate::{API, AccessToken, ChatError, HttpRequest, Method, SseEvents, ToolBox, Transport};

/// What the model is told about itself and the tools.
pub const INSTRUCTIONS: &str = "You are the assistant in Knotes, a notes app. You answer questions about the user's vault: \
Markdown notes and PDFs. Use the vault tools to find and read what you need before answering; don't guess what the notes say. \
Search gives fragments; read a note's blocks or a PDF's pages for more. When you use a note or a PDF, link it as an Obsidian \
wiki link with its path, like [[Projects/Radar]] for Projects/Radar.md or [[Papers/budget.pdf#page=3]] for a PDF's page, and \
to the section when you know its heading (search and reads give it), like [[Projects/Radar#Budget]]; the app makes them clickable \
and opens them there. For to-dos, list_tasks: it sees only checkbox items, so for plans, appointments and what is coming up \
also read the recent notes (list_notes changed in the last week or two), where they are often written as plain text. For what \
is connected with a note, related. Search matches words by their start, in any language: search by a word's stem to catch its forms \
(\"прал\" finds пралка and пральна, \"гаранті\" finds гарантія and гарантійний). When a search finds nothing, try the stem, other words \
or synonyms, or find_by_title and list_notes, before saying the vault does not have it. Search in the language the notes are \
written in (their titles show it), translating the user's words when they ask in another. For a question about several notes or a \
period, look through all of them before answering. Search a date first as the user says it (\"18 листопада\", \
\"жовтень\"); if nothing comes, try the forms notes often use (2026-11-18, 18.11.2026, 18.11, 2026-10). Prefer figures written in the notes (readings, tables, receipts) \
to working them out from others; if you must estimate, say so and from what. When notes disagree, say so, which says what and when each last changed (modified). When the user says \
\"this\" or \"here\" and no document is open, ask which note they mean. Tools give results a page at a time: when a result has next_offset, call the tool again with it as \
offset to see more; read a long note or PDF on from next_block or next_page. Know when to stop. Search by meaning (finds marked `meaning`) already tries other wordings and languages, \
so do not try synonyms one by one yourself. If two or three searches bring nothing about what was asked (`words_found` false, the \
finds by meaning about something else), the vault does not have it: say so at once, naming what is nearest if it helps, and never \
spend more than eight tool calls on something not found. A question about several notes, a list or a period is different: read \
every note that bears on it. Answer in the language the user writes in. Be concise.";

/// What the model is told when it summarizes a conversation's older part.
pub const SUMMARY_INSTRUCTIONS: &str = "You summarize the earlier part of a conversation between a user and the assistant of \
Knotes, a notes app, so the assistant can go on without it. Keep what matters for going on: the user's questions and aims, the \
facts found with the notes or PDFs they come from (as [[path]] links), the answers given, decisions, and what is still open. \
Leave out greetings and the tools' raw output. Write in the language of the conversation, as short notes, at most 400 words.";

/// A reply's output items, its input's tokens and how many of them came from the cache.
type Reply = (Vec<Value>, Option<u64>, Option<u64>);

/// The tools' namespace (the route takes function tools only in one).
const NAMESPACE: &str = "vault";
/// Replies with tool calls one turn may take before it gives up.
const MAX_STEPS: usize = 16;
/// Tool calls in one turn after which the model is reminded to answer what it has not found:
/// told so at the start, it still searched on through synonyms, folders and days (22–26 calls
/// for what the vault does not hold); told when it gets there, it stops. At 8 it had often made
/// 11 by then, calling four at once.
const REMIND_AT: usize = 6;

/// What a turn shows on the way.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// Text of the reply, a piece at a time.
    Text(String),
    /// The model calls a tool.
    ToolCall { name: String, arguments: Value },
    ToolDone { name: String, failed: bool },
    /// The conversation's older part is being summarized first.
    Compacting,
}

/// One conversation's requests: the model, a key that lets OpenAI reuse the conversation's
/// start from its cache, how long to wait before sending again when OpenAI was unavailable or
/// the network failed (one wait a try), and the size (tokens) past which the conversation's
/// older part is summarized (`compact_at`).
pub struct Chat<'a> {
    pub transport: &'a dyn Transport,
    pub token: &'a dyn AccessToken,
    pub model: String,
    pub cache_key: String,
    pub retry_after: Vec<Duration>,
    pub compact_at: u64,
    /// The local day, as the model is told it ("Saturday, 2026-11-14 (UTC+02:00)").
    pub today: String,
    /// How hard the model reasons over the user's questions (`reasoning.effort`); none, its own
    /// default. Summaries are left at the default.
    pub effort: Option<String>,
}

impl Chat<'_> {
    /// Sends the user's `text` (with the document they have open) after the conversation's
    /// `items`, runs the tools the model calls until it answers, and adds it all to `items`.
    /// A conversation grown past `compact_at` gets its older part summarized first (the summary
    /// kept even if the turn then fails: it covers only items from before it); its size comes
    /// from OpenAI's count of the last request when known, else from an estimate. A turn that
    /// fails or is called off leaves `items` as they were.
    #[allow(clippy::too_many_arguments)]
    pub fn send(&self, items: &mut Vec<Value>, memory: &mut Memory, text: &str, open: Option<&str>, tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<(), ChatError> {
        let (before, tokens, summary) = (items.len(), memory.tokens, memory.summary.clone());
        if memory.preamble.is_none() {
            memory.preamble = Some(self.preamble(tools));
        }
        let result = self.turn(items, memory, text, open, tools, cancel, on_event);
        if result.is_err() {
            items.truncate(before);
            memory.tokens = if memory.summary == summary { tokens } else { None };
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn turn(&self, items: &mut Vec<Value>, memory: &mut Memory, text: &str, open: Option<&str>, tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<(), ChatError> {
        let before = items.len();
        if let Some(path) = open.filter(|p| !tools.excluded(p)) {
            let words = format!("The user has {path} open in Knotes; \"this note\", \"here\" and the like mean it.");
            items.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": words}]}));
        }
        items.push(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}));
        let size = match memory.tokens {
            Some(tokens) => tokens + estimate_tokens(&items[before..]) as u64,
            None => estimate_tokens(&context(items, memory)) as u64,
        };
        if size > self.compact_at {
            self.compact(items, &mut memory.summary, cancel, on_event)?;
            // The cached start is gone with the summary anyway: the day and the vault anew.
            memory.preamble = Some(self.preamble(tools));
            memory.tokens = None;
        }
        let mut made = 0;
        for _ in 0..MAX_STEPS {
            let (output, input_tokens, cached) = self.respond(&context(items, memory), tools, cancel, on_event)?;
            if cached.is_some() {
                memory.cached = cached;
            }
            let calls: Vec<Value> = output.iter().filter(|i| i["type"] == "function_call").cloned().collect();
            let kept: Vec<Value> = output.iter().filter_map(kept).collect();
            if let Some(input_tokens) = input_tokens {
                memory.tokens = Some(input_tokens + estimate_tokens(&kept) as u64);
            }
            items.extend(kept);
            if calls.is_empty() {
                return Ok(());
            }
            let before = made;
            for call in calls {
                made += 1;
                if cancel.load(Ordering::Relaxed) {
                    return Err(ChatError::Cancelled);
                }
                let name = call["name"].as_str().unwrap_or_default();
                let name = name.rsplit_once('.').map_or(name, |(_, n)| n).to_string();
                let output = match serde_json::from_str::<Value>(call["arguments"].as_str().unwrap_or("{}")) {
                    Ok(arguments) => {
                        on_event(ChatEvent::ToolCall { name: name.clone(), arguments: arguments.clone() });
                        tools.call(&name, &arguments).map_err(|e| e.to_string())
                    }
                    Err(e) => Err(format!("arguments are not JSON: {e}")),
                };
                on_event(ChatEvent::ToolDone { name, failed: output.is_err() });
                let output = output.unwrap_or_else(|e| json!({"error": e})).to_string();
                items.push(json!({"type": "function_call_output", "call_id": call["call_id"], "output": output}));
            }
            if before < REMIND_AT && made >= REMIND_AT {
                let words = format!("{made} tool calls so far. If what was asked has not turned up, the vault most likely does not have it: answer now, saying so and naming what is nearest. Go on only to read the notes a list or a period needs.");
                items.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": words}]}));
            }
        }
        Err(ChatError::TooManySteps)
    }

    /// Summarizes the items before the last turns (and the summary before, if any) into a new
    /// summary covering them.
    fn compact(&self, items: &[Value], summary: &mut Option<Summary>, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<(), ChatError> {
        let from = summary.as_ref().map_or(0, |s| s.covers);
        let until = recent_start(&turn_starts(items));
        if until <= from {
            return Ok(());
        }
        on_event(ChatEvent::Compacting);
        let mut text = String::new();
        if let Some(summary) = summary.as_ref() {
            text.push_str(&format!("The summary so far:\n{}\n\n", summary.text));
        }
        text.push_str(&format!("The conversation to summarize:\n\n{}", transcript(&items[from..until])));
        let body = json!({
            "model": self.model,
            "instructions": SUMMARY_INSTRUCTIONS,
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}],
            "store": false,
            "stream": true,
        });
        let (output, _, _) = self.request(&body, cancel, &mut |_| {})?;
        let words: String = output.iter().filter(|i| i["type"] == "message").filter_map(|i| i["content"].as_array()).flatten().filter_map(|c| c["text"].as_str()).collect();
        if words.trim().is_empty() {
            return Err(ChatError::Failed("the summary came back empty".into()));
        }
        *summary = Some(Summary { covers: until, text: words.trim().to_string() });
        Ok(())
    }

    /// What the model is told of the day and the vault, set once a conversation.
    fn preamble(&self, tools: &dyn ToolBox) -> String {
        format!("Today is {}. {}", self.today, tools.overview()).trim_end().to_string()
    }

    /// One streamed reply to `input`, with the vault's tools, its input's tokens and how many
    /// of them came from the cache.
    fn respond(&self, input: &[Value], tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<Reply, ChatError> {
        let functions: Vec<Value> = tools
            .specs()
            .into_iter()
            .map(|s| json!({"type": "function", "name": s.name, "description": s.description, "parameters": s.parameters, "strict": false}))
            .collect();
        let mut body = json!({
            "model": self.model,
            "instructions": INSTRUCTIONS,
            "input": input,
            "tools": [{"type": "namespace", "name": NAMESPACE, "description": "Read the user's Knotes vault.", "tools": functions}],
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "include": ["reasoning.encrypted_content"],
            "prompt_cache_key": self.cache_key,
            "store": false,
            "stream": true,
        });
        if let Some(effort) = &self.effort {
            body["reasoning"] = json!({"effort": effort});
        }
        self.request(&body, cancel, on_event)
    }

    /// Sends `body` to the Responses API (again after a wait, while OpenAI is unavailable or the
    /// network fails before a reply starts) and reads the streamed reply: its output items once
    /// it says it is complete, how many tokens its input took and how many came from the cache
    /// (OpenAI's counts).
    fn request(&self, body: &Value, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<Reply, ChatError> {
        let request = HttpRequest {
            method: Method::Post,
            url: format!("{API}/responses"),
            headers: vec![("Content-Type".into(), "application/json".into()), ("Accept".into(), "text/event-stream".into())],
            body: body.to_string().into_bytes(),
        };
        let mut tries = self.retry_after.iter();
        let response = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ChatError::Cancelled);
            }
            let result = send_authorized(self.transport, self.token, request.clone()).and_then(|r| if (200..300).contains(&r.status) { Ok(r) } else { Err(error_of(r)) });
            match (result, tries.next()) {
                (Err(ChatError::Unavailable(_) | ChatError::Network(_)), Some(&wait)) => pause(wait, cancel),
                (result, _) => break result?,
            }
        };
        let mut done = Vec::new();
        for event in SseEvents::new(response.body) {
            if cancel.load(Ordering::Relaxed) {
                return Err(ChatError::Cancelled);
            }
            let event = event.map_err(|e| ChatError::Network(e.to_string()))?;
            let Ok(data) = serde_json::from_str::<Value>(&event.data) else { continue };
            match data.get("type").and_then(Value::as_str).unwrap_or(&event.event) {
                "response.output_text.delta" => on_event(ChatEvent::Text(data["delta"].as_str().unwrap_or_default().to_string())),
                "response.output_item.done" => done.push(data["item"].clone()),
                "response.completed" => {
                    let output = data["response"]["output"].as_array().filter(|o| !o.is_empty()).cloned();
                    let usage = &data["response"]["usage"];
                    return Ok((output.unwrap_or(done), usage["input_tokens"].as_u64(), usage["input_tokens_details"]["cached_tokens"].as_u64()));
                }
                "response.failed" => return Err(ChatError::from_openai(&data["response"]["error"], 0)),
                "response.incomplete" => {
                    let reason = data["response"]["incomplete_details"]["reason"].as_str().unwrap_or("unknown");
                    return Err(ChatError::Incomplete(reason.to_string()));
                }
                "error" => return Err(ChatError::from_openai(data.get("error").filter(|e| e.is_object()).unwrap_or(&data), 0)),
                _ => {}
            }
        }
        Err(ChatError::Interrupted)
    }
}

/// Waits `wait`, or less if the turn is called off meanwhile.
fn pause(wait: Duration, cancel: &AtomicBool) {
    let until = Instant::now() + wait;
    while Instant::now() < until && !cancel.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())));
    }
}

/// An output item as the conversation keeps it to send back: without the ids the server
/// keeps none of (`store: false`), a message as its text, reasoning only when sealed for
/// sending back; other items dropped.
fn kept(item: &Value) -> Option<Value> {
    match item["type"].as_str()? {
        "message" => {
            let text: String = item["content"].as_array()?.iter().filter_map(|c| c["text"].as_str()).collect();
            Some(json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}))
        }
        "reasoning" if item.get("encrypted_content").is_some_and(Value::is_string) => Some(without_ids(item)),
        "function_call" => Some(without_ids(item)),
        _ => None,
    }
}

fn without_ids(item: &Value) -> Value {
    let mut item = item.clone();
    if let Some(fields) = item.as_object_mut() {
        fields.remove("id");
        fields.remove("status");
    }
    item
}
