//! What a long conversation sends (ADR 0027): its summary in place of the items it covers,
//! reasoning only from the turn under way, and old tool results cut short.

use serde_json::{Value, json};

/// The input past which a request costs twice as much (OpenAI's pricing of GPT-5.6 and its
/// kin, whose windows reach 1 050 000): no conversation is let grow past it.
pub const PRICED_WINDOW: u64 = 272_000;

/// The size (tokens) past which a conversation's older part is summarized: 90 % of the
/// model's window, as Codex does, the window taken no larger than `PRICED_WINDOW`; a model
/// whose window is not known is taken to have that one.
pub fn compact_at(context_window: Option<u64>) -> u64 {
    context_window.unwrap_or(PRICED_WINDOW).min(PRICED_WINDOW) * 9 / 10
}

/// What a conversation remembers of its size: the summary standing in for its first items,
/// and how many tokens it took when last sent (the last request's input as OpenAI counted it,
/// with what came back); unknown before the first reply and after a summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Memory {
    pub summary: Option<Summary>,
    pub tokens: Option<u64>,
}
/// The last turns sent whole, their tool results uncut (the one under way among them).
pub const KEEP_TURNS: usize = 2;
/// An older tool result past this many characters is cut.
const OLD_RESULT: usize = 1_500;

/// What the model gets in place of a conversation's first `covers` items once it grew long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub covers: usize,
    pub text: String,
}

/// The items to send for `items`: the summary (as a developer message) in place of the
/// items it covers; reasoning only from the last turn (it carries the turn's tool calls on);
/// tool results before the last `KEEP_TURNS` turns cut short.
pub fn context(items: &[Value], summary: Option<&Summary>) -> Vec<Value> {
    let from = summary.map_or(0, |s| s.covers.min(items.len()));
    let starts = turn_starts(items);
    let current = starts.last().copied().unwrap_or(0);
    let recent = recent_start(&starts);
    let mut out = Vec::with_capacity(items.len() - from + 1);
    if let Some(summary) = summary {
        let words = format!("A summary of this conversation's earlier part (its messages are left out):\n{}", summary.text);
        out.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": words}]}));
    }
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
