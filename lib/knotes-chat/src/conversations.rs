use std::fs;
use std::io;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::Conversation;

const INDEX: &str = "index.json";

/// A conversation as listed, without its items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEntry {
    pub id: String,
    pub title: String,
    pub updated: u64,
}

/// A vault's conversations, a file each in a folder beside its index, with a list of them
/// (so listing reads one small file).
pub struct Conversations {
    dir: PathBuf,
}

impl Conversations {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Newest first.
    pub fn list(&self) -> Vec<ConversationEntry> {
        let value: Value = fs::read_to_string(self.dir.join(INDEX)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut entries: Vec<ConversationEntry> = value
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|e| {
                        Some(ConversationEntry { id: e["id"].as_str()?.to_string(), title: e["title"].as_str().unwrap_or_default().to_string(), updated: e["updated"].as_u64().unwrap_or(0) })
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by_key(|e| std::cmp::Reverse(e.updated));
        entries
    }

    pub fn load(&self, id: &str) -> Option<Conversation> {
        if !is_id(id) {
            return None;
        }
        let text = fs::read_to_string(self.dir.join(format!("{id}.json"))).ok()?;
        Conversation::from_json(&serde_json::from_str(&text).ok()?)
    }

    pub fn save(&self, conversation: &Conversation) -> io::Result<()> {
        if !is_id(&conversation.id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a conversation id"));
        }
        fs::create_dir_all(&self.dir)?;
        write(&self.dir.join(format!("{}.json", conversation.id)), &conversation.to_json().to_string())?;
        let mut entries = self.list();
        entries.retain(|e| e.id != conversation.id);
        entries.push(ConversationEntry { id: conversation.id.clone(), title: conversation.title.clone(), updated: conversation.updated });
        self.save_list(&entries)
    }

    pub fn delete(&self, id: &str) {
        if !is_id(id) {
            return;
        }
        let _ = fs::remove_file(self.dir.join(format!("{id}.json")));
        let mut entries = self.list();
        entries.retain(|e| e.id != id);
        let _ = self.save_list(&entries);
    }

    fn save_list(&self, entries: &[ConversationEntry]) -> io::Result<()> {
        let list: Vec<Value> = entries.iter().map(|e| json!({"id": e.id, "title": e.title, "updated": e.updated})).collect();
        write(&self.dir.join(INDEX), &Value::Array(list).to_string())
    }
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Written to a temporary file and renamed, so a crash never leaves half a file.
fn write(path: &std::path::Path, text: &str) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)
}
