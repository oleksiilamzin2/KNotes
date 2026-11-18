use std::borrow::Cow;
use std::collections::HashMap;

use serde_json::{Map, Value};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

use crate::{Space, SpaceColor};

/// What a folder's rules say of every space.
pub const SHARED: &str = "*";

/// A vault's spaces and which folders are in them (ADR 0030). The root's rules list the spaces
/// (`"spaces": [{"id", "name", "color"}]`); a folder's rules name its space (`"space": "id"`,
/// `"*"` for every space, or `null` for none), which its subfolders take unless one names
/// another. The root is in no space. Folders are compared composed (NFC).
#[derive(Debug, Clone, Default)]
pub struct Spaces {
    list: Vec<Space>,
    /// Each folder whose rules say something of its space.
    folders: HashMap<String, Rule>,
}

/// What a folder's rules say: a space (its index), every space, or none (`null`, or an id no
/// space has).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    In(usize),
    Shared,
    None,
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
            let rule = match rules.get("space") {
                Some(Value::String(id)) if id == SHARED => Rule::Shared,
                Some(Value::String(id)) => list.iter().position(|s| s.id == *id).map_or(Rule::None, Rule::In),
                Some(Value::Null) => Rule::None,
                _ => continue,
            };
            folders.insert(composed(folder).into_owned(), rule);
        }
        Self { list, folders }
    }

    /// From each folder's rules file as written; a file that is not a JSON object is skipped.
    pub fn from_texts(rules: &[(String, String)]) -> Self {
        let parsed: Vec<(&str, Map<String, Value>)> = rules.iter().filter_map(|(folder, text)| Some((folder.as_str(), serde_json::from_str(text).ok()?))).collect();
        Self::from_rules(parsed.iter().map(|(folder, rules)| (*folder, rules)))
    }

    /// What a folder under `parent` must say to be in the space `id` (none: in no space): nothing
    /// when its parent's is that already, unless the parent is in every space (then the folder
    /// says its own).
    pub fn says_for<'a>(&self, parent: &str, id: Option<&'a str>) -> crate::FolderSpace<'a> {
        let inherited = (!self.shared(parent)).then(|| self.of_folder(parent).map(|space| space.id.as_str()));
        match id {
            _ if inherited == Some(id) => crate::FolderSpace::Inherit,
            Some(id) => crate::FolderSpace::In(id),
            None => crate::FolderSpace::None,
        }
    }

    /// What a folder under `parent` must say to be in every space: nothing when its parent is.
    pub fn says_for_every(&self, parent: &str) -> crate::FolderSpace<'static> {
        if self.shared(parent) { crate::FolderSpace::Inherit } else { crate::FolderSpace::Shared }
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

    /// The rule a folder follows: the nearest of it and its parents' that says one.
    fn rule(&self, folder: &str) -> Rule {
        let folder = composed(folder);
        let mut at = folder.as_ref();
        while !at.is_empty() {
            if let Some(rule) = self.folders.get(at) {
                return *rule;
            }
            at = at.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
        Rule::None
    }

    /// The one space of a folder: the nearest of it and its parents that names one; none for a
    /// folder in every space.
    pub fn of_folder(&self, folder: &str) -> Option<&Space> {
        match self.rule(folder) {
            Rule::In(i) => Some(&self.list[i]),
            Rule::Shared | Rule::None => None,
        }
    }

    /// Whether the folder is in every space.
    pub fn shared(&self, folder: &str) -> bool {
        self.rule(folder) == Rule::Shared
    }

    /// Whether the folder is in the space `id`: its own, or every space.
    pub fn holds_folder(&self, folder: &str, id: &str) -> bool {
        match self.rule(folder) {
            Rule::In(i) => self.list[i].id == id,
            Rule::Shared => true,
            Rule::None => false,
        }
    }

    /// Whether the document at `path` is in the space `id` (its folder is).
    pub fn holds(&self, path: &str, id: &str) -> bool {
        path.rsplit_once('/').is_some_and(|(folder, _)| self.holds_folder(folder, id))
    }

    /// What the folder's own rules say: a space, none (`null` or an unknown id; every space
    /// counts too, as no one space), or nothing at all (`None`: it takes its parent's).
    pub fn own(&self, folder: &str) -> Option<Option<&Space>> {
        self.folders.get(composed(folder).as_ref()).map(|rule| match rule {
            Rule::In(i) => Some(&self.list[*i]),
            Rule::Shared | Rule::None => None,
        })
    }

    /// Whether the folder's own rules put it in every space.
    pub fn own_shared(&self, folder: &str) -> bool {
        self.folders.get(composed(folder).as_ref()) == Some(&Rule::Shared)
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
