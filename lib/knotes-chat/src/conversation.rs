use serde_json::{Value, json};

use crate::Summary;
use crate::crypto::random_bytes;
use crate::token_endpoint::now;

/// A title past this many characters is cut.
const TITLE: usize = 60;

/// One conversation: what it is called and with which model, and the items sent back to the
/// model each turn (ADR 0027).
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    /// Hex digits: a file name as it is.
    pub id: String,
    pub title: String,
    pub model: String,
    /// When it last changed (seconds since 1970).
    pub updated: u64,
    pub items: Vec<Value>,
    /// What the model gets in place of the conversation's first items once it grew long.
    pub summary: Option<Summary>,
}

/// What the panel shows of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    User(String),
    Assistant(String),
    Tool { name: String, arguments: Value },
}

impl Conversation {
    pub fn new(model: &str) -> Self {
        let tail: String = random_bytes::<4>().iter().map(|b| format!("{b:02x}")).collect();
        Self { id: format!("{:x}{tail}", now()), title: String::new(), model: model.to_string(), updated: now(), items: Vec::new(), summary: None }
    }

    /// Names an unnamed conversation by the first line of its first question.
    pub fn name_from_first_question(&mut self) {
        if !self.title.is_empty() {
            return;
        }
        let Some(Shown::User(question)) = self.shown().into_iter().find(|s| matches!(s, Shown::User(_))) else { return };
        let line = question.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
        self.title = if line.chars().count() > TITLE { format!("{}…", line.chars().take(TITLE).collect::<String>().trim_end()) } else { line.to_string() };
    }

    pub fn shown(&self) -> Vec<Shown> {
        self.shown_at().into_iter().map(|(_, shown)| shown).collect()
    }

    /// What the panel shows, each with the index of its item.
    pub fn shown_at(&self) -> Vec<(usize, Shown)> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match (item["type"].as_str()?, item["role"].as_str()) {
                ("message", Some("user")) => Some((i, Shown::User(text_of(item)))),
                ("message", Some("assistant")) => Some((i, Shown::Assistant(text_of(item)))),
                ("function_call", _) => {
                    let name = item["name"].as_str()?;
                    let arguments = serde_json::from_str(item["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
                    Some((i, Shown::Tool { name: name.rsplit_once('.').map_or(name, |(_, n)| n).to_string(), arguments }))
                }
                _ => None,
            })
            .collect()
    }

    pub(crate) fn to_json(&self) -> Value {
        let summary = self.summary.as_ref().map(|s| json!({"covers": s.covers, "text": s.text}));
        json!({"id": self.id, "title": self.title, "model": self.model, "updated": self.updated, "items": self.items, "summary": summary})
    }

    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            id: value["id"].as_str()?.to_string(),
            title: value["title"].as_str().unwrap_or_default().to_string(),
            model: value["model"].as_str().unwrap_or_default().to_string(),
            updated: value["updated"].as_u64().unwrap_or(0),
            items: value["items"].as_array().cloned().unwrap_or_default(),
            summary: value["summary"]["text"].as_str().map(|text| Summary { covers: value["summary"]["covers"].as_u64().unwrap_or(0) as usize, text: text.to_string() }),
        })
    }
}

fn text_of(item: &Value) -> String {
    item["content"].as_array().map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect()).unwrap_or_default()
}
