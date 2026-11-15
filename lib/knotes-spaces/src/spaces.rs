use std::borrow::Cow;
use std::collections::HashMap;

use serde_json::{Map, Value};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

use crate::{Space, SpaceColor};

/// A vault's spaces and which folders are in them (ADR 0030). The root's rules list the spaces
/// (`"spaces": [{"id", "name", "color"}]`); a folder's rules name its space (`"space": "id"`,
/// or `null` for none), which its subfolders take unless one names another. The root is in no
/// space. Folders are compared composed (NFC).
#[derive(Debug, Clone, Default)]
pub struct Spaces {
    list: Vec<Space>,
    /// Each folder that names a space: the space's index, or none (`null`, or an id no space has).
    folders: HashMap<String, Option<usize>>,
}

impl Spaces {
    /// From each folder's rules (the root's is the folder ""), already parsed.
    pub fn from_rules<'a>(rules: impl IntoIterator<Item = (&'a str, &'a Map<String, Value>)>) -> Self {
        let rules: Vec<(&str, &Map<String, Value>)> = rules.into_iter().collect();
        let mut list: Vec<Space> = Vec::new();
        if let Some(Value::Array(spaces)) = rules.iter().find(|(folder, _)| folder.is_empty()).and_then(|(_, root)| root.get("spaces")) {
            for space in spaces {
                let Some(id) = space.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) else { continue };
                if list.iter().any(|s| s.id == id) {
                    continue;
                }
                let name = space.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or(id);
                let color = SpaceColor::from_name(space.get("color").and_then(Value::as_str).unwrap_or_default());
                list.push(Space { id: id.to_string(), name: name.to_string(), color });
            }
        }
        let mut folders = HashMap::new();
        for (folder, rules) in rules.iter().filter(|(folder, _)| !folder.is_empty()) {
            let space = match rules.get("space") {
                Some(Value::String(id)) => list.iter().position(|s| s.id == *id),
                Some(Value::Null) => None,
                _ => continue,
            };
            folders.insert(composed(folder).into_owned(), space);
        }
        Self { list, folders }
    }

    /// From each folder's rules file as written; a file that is not a JSON object is skipped.
    pub fn from_texts(rules: &[(String, String)]) -> Self {
        let parsed: Vec<(&str, Map<String, Value>)> = rules.iter().filter_map(|(folder, text)| Some((folder.as_str(), serde_json::from_str(text).ok()?))).collect();
        Self::from_rules(parsed.iter().map(|(folder, rules)| (*folder, rules)))
    }

    /// The spaces, in the order the root lists them.
    pub fn list(&self) -> &[Space] {
        &self.list
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Space> {
        self.list.iter().find(|s| s.id == id)
    }

    /// The space of a folder: the nearest of it and its parents that names one.
    pub fn of_folder(&self, folder: &str) -> Option<&Space> {
        let folder = composed(folder);
        let mut at = folder.as_ref();
        while !at.is_empty() {
            if let Some(space) = self.folders.get(at) {
                return space.map(|i| &self.list[i]);
            }
            at = at.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
        None
    }

    /// What the folder's own rules say: a space, none (`null` or an unknown id), or nothing at
    /// all (`None`: it takes its parent's).
    pub fn own(&self, folder: &str) -> Option<Option<&Space>> {
        self.folders.get(composed(folder).as_ref()).map(|space| space.map(|i| &self.list[i]))
    }

    /// The space of a document: its folder's.
    pub fn of(&self, path: &str) -> Option<&Space> {
        self.of_folder(path.rsplit_once('/').map_or("", |(folder, _)| folder))
    }
}

/// `text` composed, borrowed when it is already.
fn composed(text: &str) -> Cow<'_, str> {
    if is_nfc_quick(text.chars()) == IsNormalized::Yes { Cow::Borrowed(text) } else { Cow::Owned(text.nfc().collect()) }
}
