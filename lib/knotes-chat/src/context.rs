//! What a conversation sends (ADR 0027): the day and the vault as they were when it started,
//! its summary in place of the items it covers once it grew long, and the rest: as it is kept
//! where OpenAI caches the conversation (each request then starts as the one before did, and the
//! cache covers that start), else with old tool results cut and past reasoning left out, which
//! sends fewer tokens.

use serde_json::{Map, Value, json};

/// The input past which a request costs twice as much (OpenAI's pricing of GPT-5.6 and its
/// kin, whose windows reach 1 050 000): no conversation is let grow past it.
pub const PRICED_WINDOW: u64 = 272_000;

/// The size (tokens) past which a conversation's older part is summarized: 90 % of the
/// model's window, as Codex does, the window taken no larger than `PRICED_WINDOW`; a model
/// whose window is not known is taken to have that one.
pub fn compact_at(context_window: Option<u64>) -> u64 {
    context_window.unwrap_or(PRICED_WINDOW).min(PRICED_WINDOW) * 9 / 10
}

/// What a conversation keeps beside its items: what the model is told of the day and the vault
/// (set when it starts, again only with a summary, so the requests' start stays the same); the
/// summary standing in for its first items; how many tokens it took when last sent (the last
/// request's input as OpenAI counted it, with what came back; unknown before the first reply
/// and after a summary); and how many of those OpenAI took from its cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Memory {
    pub preamble: Option<String>,
    pub summary: Option<Summary>,
    pub tokens: Option<u64>,
    pub cached: Option<u64>,
}
/// The last turns a summary leaves out (the one under way among them).
pub const KEEP_TURNS: usize = 2;
/// A tool result past this many characters is cut where nothing is cached, and in what the
/// model summarizes.
const OLD_RESULT: usize = 1_500;

/// What the model gets in place of a conversation's first `covers` items once it grew long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub covers: usize,
    pub text: String,
}

/// The items to send for `items`: the preamble and the summary (developer messages), then the
/// items the summary does not cover. As they are once OpenAI has cached at least half of a
/// request of this conversation (the cache then covers its history); else reasoning only from
/// the last turn, and tool results before the last `KEEP_TURNS` turns cut short. (On 2026-11-14
/// the Sign in with ChatGPT route cached at most the start every request shares, tools and
/// instructions, never a conversation's history, not even a request sent again unchanged.)
pub fn context(items: &[Value], memory: &Memory) -> Vec<Value> {
    let from = memory.summary.as_ref().map_or(0, |s| s.covers.min(items.len()));
    let developer = |words: &str| json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": words}]});
    let mut out = Vec::with_capacity(items.len() - from + 2);
    if let Some(preamble) = &memory.preamble {
        out.push(developer(preamble));
    }
    if let Some(summary) = &memory.summary {
        out.push(developer(&format!("A summary of this conversation's earlier part (its messages are left out):\n{}", summary.text)));
    }
    if memory.cached.is_some_and(|c| c > 0 && memory.tokens.is_some_and(|t| c * 2 >= t)) {
        out.extend(items[from..].iter().cloned());
        return out;
    }
    let starts = turn_starts(items);
    let current = starts.last().copied().unwrap_or(0);
    let recent = recent_start(&starts);
    for (i, item) in items.iter().enumerate().skip(from) {
        match item["type"].as_str() {
            Some("reasoning") if i < current => {}
            Some("function_call_output") if i < recent => {
                let mut item = item.clone();
                if let Some(output) = item["output"].as_str().filter(|o| o.chars().count() > OLD_RESULT) {
                    let cut: String = output.chars().take(OLD_RESULT).collect();
                    item["output"] = json!(format!("{cut}… [cut: call the tool again for the rest]"));
                }
                out.push(item);
            }
            _ => out.push(item.clone()),
        }
    }
    out
}

/// `items` as they may be sent now: without what the user has since kept from the AI
/// (`excluded`: tagged `#no-ai`, a folder's `"ai": false`), which earlier turns' tool results may
/// hold. A result about a document now kept (a read, an outline: its `path`) is withheld whole;
/// an entry naming one (a search's find, a listed note, a link) is taken out of its result. The
/// items stay as many and in their places (a summary covers them by count), those nothing
/// changed in byte for byte (the cache), and the conversation kept keeps them all: shared again,
/// a document's results go again. The answers given and a summary keep what they say.
pub fn withhold(items: &[Value], excluded: &dyn Fn(&str) -> bool) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            let output = (item["type"] == "function_call_output").then(|| item["output"].as_str()).flatten();
            let Some(result) = output.and_then(|o| serde_json::from_str::<Value>(o).ok()) else { return item.clone() };
            let left = without_kept(&result, excluded);
            if left.as_ref() == Some(&result) {
                return item.clone();
            }
            let mut item = item.clone();
            item["output"] = json!(match left {
                Some(left) => left.to_string(),
                None => json!({"withheld": "about a document the user has since kept from the AI: do not use or repeat what was given of it"}).to_string(),
            });
            item
        })
        .collect()
}

/// `value` without the objects whose `path` is a document kept: none when it is one.
fn without_kept(value: &Value, excluded: &dyn Fn(&str) -> bool) -> Option<Value> {
    match value {
        Value::Object(fields) => {
            if fields.get("path").and_then(Value::as_str).is_some_and(excluded) {
                return None;
            }
            let mut left = Map::new();
            for (key, field) in fields {
                if let Some(field) = without_kept(field, excluded) {
                    left.insert(key.clone(), field);
                }
            }
            Some(Value::Object(left))
        }
        Value::Array(list) => Some(Value::Array(list.iter().filter_map(|v| without_kept(v, excluded)).collect())),
        other => Some(other.clone()),
    }
}

/// A rough count of the tokens `input` takes: its JSON's bytes over four (Cyrillic, two bytes
/// a letter, comes out a little over; erring that way summarizes a little early).
pub fn estimate_tokens(input: &[Value]) -> usize {
    input.iter().map(|item| item.to_string().len()).sum::<usize>() / 4
}

/// Where each turn starts: at the user's message, or at the developer message naming the
/// document open just before it.
pub(crate) fn turn_starts(items: &[Value]) -> Vec<usize> {
    let role = |i: usize| items.get(i).filter(|item| item["type"] == "message").and_then(|item| item["role"].as_str());
    (0..items.len()).filter(|&i| role(i) == Some("user")).map(|i| if i > 0 && role(i - 1) == Some("developer") { i - 1 } else { i }).collect()
}

/// Where the last `KEEP_TURNS` turns start.
pub(crate) fn recent_start(starts: &[usize]) -> usize {
    starts.len().checked_sub(KEEP_TURNS).map_or(0, |i| starts[i])
}

/// `items` as plain text, for the model to summarize.
pub(crate) fn transcript(items: &[Value]) -> String {
    let text_of = |item: &Value| -> String { item["content"].as_array().map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect()).unwrap_or_default() };
    let mut out = String::new();
    for item in items {
        let line = match (item["type"].as_str(), item["role"].as_str()) {
            (Some("message"), Some("user")) => format!("User: {}", text_of(item)),
            (Some("message"), Some("assistant")) => format!("Assistant: {}", text_of(item)),
            (Some("message"), _) => format!("(Context: {})", text_of(item)),
            (Some("function_call"), _) => format!("Tool call: {} {}", item["name"].as_str().unwrap_or_default(), item["arguments"].as_str().unwrap_or_default()),
            (Some("function_call_output"), _) => {
                let output = item["output"].as_str().unwrap_or_default();
                let cut: String = output.chars().take(OLD_RESULT).collect();
                format!("Tool result: {cut}{}", if cut.len() < output.len() { "…" } else { "" })
            }
            _ => continue,
        };
        out.push_str(&line);
        out.push_str("\n\n");
    }
    out
}
