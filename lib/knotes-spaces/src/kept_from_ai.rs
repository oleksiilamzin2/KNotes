//! What the user keeps from the AI (ADR 0027, 0036): notes tagged `#no-ai`; folders whose
//! `.knotes.json` says `"ai": false`, with their subfolders unless one says `"ai": true`; and
//! files listed in their folder's `.knotes.json` as `"files": {"name": {"ai": false}}` (PDFs and
//! other files, which have no tags; `"ai": true` there shares a file its folder keeps). The
//! nearest setting wins: a note's tag, then the file's own, then its folders' from the nearest
//! up. Paths are compared composed (macOS writes some names decomposed).

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

/// The tag a note is kept from the AI with.
pub const NO_AI_TAG: &str = "no-ai";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptFromAi {
    /// The notes tagged `#no-ai`.
    notes: HashSet<String>,
    /// Each folder's own `ai`, by its path (`""`: the vault's root).
    folders: HashMap<String, bool>,
    /// Each file's own `ai`, by its path.
    files: HashMap<String, bool>,
}

fn composed(text: &str) -> String {
    text.nfc().collect()
}

/// The folder `path` is in (`""` for one at the root).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

impl KeptFromAi {
    /// From the notes tagged `#no-ai` and the rules files, as JSON objects by folder.
    pub fn new<'a>(tagged: impl IntoIterator<Item = &'a str>, rules: impl IntoIterator<Item = (&'a str, &'a Map<String, Value>)>) -> Self {
        let mut kept = Self { notes: tagged.into_iter().map(composed).collect(), ..Self::default() };
        for (folder, rules) in rules {
            kept.add_rules(folder, rules);
        }
        kept
    }

    /// The rules of `folder`: its own `ai`, and its files'.
    pub fn add_rules(&mut self, folder: &str, rules: &Map<String, Value>) {
        let folder = composed(folder);
        // An `ai` that is neither true nor false (`"false"` in quotes, `0`, `null`) keeps: the
        // user wrote it to say something of the AI, and kept by mistake is better than shared
        // by mistake. (It was passed over, and the folder shared.)
        let said = |value: &Value| value.as_bool().unwrap_or(false);
        if let Some(ai) = rules.get("ai").map(said) {
            self.folders.insert(folder.clone(), ai);
        }
        for (name, file) in rules.get("files").and_then(Value::as_object).into_iter().flatten() {
            if let Some(ai) = file.get("ai").map(said) {
                let path = if folder.is_empty() { composed(name) } else { format!("{folder}/{}", composed(name)) };
                self.files.insert(path, ai);
            }
        }
    }

    /// The rules file of `folder` as written. One that does not parse but names `"ai"` (a comma
    /// too many) keeps the folder: kept by mistake is better than shared by mistake.
    pub fn add_rules_text(&mut self, folder: &str, text: &str) {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(rules)) => self.add_rules(folder, &rules),
            _ if text.contains("\"ai\"") => self.keep_folder(folder),
            _ => {}
        }
    }

    /// `folder` kept whatever its rules say: they cannot be read for sure.
    pub fn keep_folder(&mut self, folder: &str) {
        self.folders.insert(composed(folder), false);
    }

    /// Whether the document at `path` is kept from the AI.
    pub fn holds(&self, path: &str) -> bool {
        let path = composed(path);
        if self.notes.contains(&path) {
            return true;
        }
        match self.files.get(&path) {
            Some(&shared) => !shared,
            None => self.folder_setting(parent(&path)) == Some(false),
        }
    }

    /// Whether the folder `folder` is kept from the AI (by its own setting, or its nearest
    /// folder's).
    pub fn holds_folder(&self, folder: &str) -> bool {
        self.folder_setting(&composed(folder)) == Some(false)
    }

    /// The nearest setting from `folder` up.
    fn folder_setting(&self, mut folder: &str) -> Option<bool> {
        loop {
            if let Some(&shared) = self.folders.get(folder) {
                return Some(shared);
            }
            if folder.is_empty() {
                return None;
            }
            folder = parent(folder);
        }
    }

    /// Whether the note at `path` is tagged `#no-ai`.
    pub fn tagged(&self, path: &str) -> bool {
        self.notes.contains(&composed(path))
    }

    /// The setting the file at `path` has of its own, if any.
    pub fn own_file(&self, path: &str) -> Option<bool> {
        self.files.get(&composed(path)).copied()
    }

    /// The setting the folder `folder` has of its own, if any.
    pub fn own_folder(&self, folder: &str) -> Option<bool> {
        self.folders.get(&composed(folder)).copied()
    }

    /// Whether nothing is kept.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.folders.values().all(|&shared| shared) && self.files.values().all(|&shared| shared)
    }
}
