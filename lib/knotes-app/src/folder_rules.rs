//! Per-folder view rules in a `.knotes.json` file inside the folder (ADR 0010, 0020).
//!
//! ```json
//! { "list": { "sort": "modified", "groupByDate": true, "includeSubfolders": true } }
//! ```
//!
//! A folder takes each field from the nearest folder (itself first, then its ancestors up to
//! the vault root) whose file sets it, and the default otherwise. Unknown keys are kept when
//! the app rewrites the file.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::{ListOption, ListSettings, SortOrder};

pub const RULES_FILE: &str = ".knotes.json";

#[derive(Debug, Clone, PartialEq)]
pub struct FolderRules {
    root: Map<String, Value>,
}

/// Fields one file sets; `None` means "inherit".
#[derive(Debug, Default, Clone, Copy)]
struct ListOverrides {
    sort: Option<SortOrder>,
    group_by_date: Option<bool>,
    include_subfolders: Option<bool>,
}

impl FolderRules {
    /// The file must hold a JSON object; values of the wrong type are ignored, not errors.
    pub fn parse(json: &str) -> Result<Self, String> {
        match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(root)) => Ok(Self { root }),
            Ok(_) => Err(format!("{RULES_FILE} must contain a JSON object")),
            Err(e) => Err(format!("{RULES_FILE}: {e}")),
        }
    }

    /// Writes all list fields, keeping every other key.
    pub fn set_list(&mut self, settings: &ListSettings) {
        self.set_option(ListOption::SortBy(settings.sort));
        self.set_option(ListOption::GroupByDate(settings.group_by_date));
        self.set_option(ListOption::IncludeSubfolders(settings.include_subfolders));
    }

    /// Writes only the field of `option`: the other fields keep following the parent folders.
    pub fn set_option(&mut self, option: ListOption) {
        let list = self.root.entry("list").or_insert_with(|| json!({}));
        if !list.is_object() {
            *list = json!({});
        }
        let list = list.as_object_mut().expect("object");
        let (key, value) = match option {
            ListOption::SortBy(SortOrder::Modified) => ("sort", json!("modified")),
            ListOption::SortBy(SortOrder::Title) => ("sort", json!("title")),
            ListOption::GroupByDate(on) => ("groupByDate", json!(on)),
            ListOption::IncludeSubfolders(on) => ("includeSubfolders", json!(on)),
        };
        list.insert(key.into(), value);
    }

    /// Pretty JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(&self.root).unwrap_or_else(|_| "{}".into());
        text.push('\n');
        text
    }

    fn list_overrides(&self) -> ListOverrides {
        let list = self.root.get("list");
        let field = |name: &str| list.and_then(|l| l.get(name));
        ListOverrides {
            sort: match field("sort").and_then(Value::as_str) {
                Some("modified") => Some(SortOrder::Modified),
                Some("title") => Some(SortOrder::Title),
                _ => None,
            },
            group_by_date: field("groupByDate").and_then(Value::as_bool),
            include_subfolders: field("includeSubfolders").and_then(Value::as_bool),
        }
    }
}

/// Folder for notes created from links (and by Cmd+N without a selected folder): the root
/// `.knotes.json` field `newNotes.folder`, else `default` (ADR 0013). `""` is the vault root.
pub fn new_notes_folder(rules: &HashMap<String, FolderRules>, default: &str) -> String {
    let set = rules.get("").and_then(|r| r.root.get("newNotes")).and_then(|n| n.get("folder")).and_then(Value::as_str);
    set.unwrap_or(default).trim_matches('/').to_owned()
}

/// List settings of `folder` (`""` is the vault root), given the rules of the folders that
/// have a `.knotes.json` file.
pub fn resolve_list_settings(folder: &str, rules: &HashMap<String, FolderRules>) -> ListSettings {
    let mut settings = ListSettings::default();
    // Root first, then each deeper ancestor, then the folder itself: later levels win.
    let mut chain = vec![""];
    let mut end = 0;
    while end < folder.len() {
        end = folder[end..].find('/').map_or(folder.len(), |i| end + i);
        chain.push(&folder[..end]);
        end += 1;
    }
    for level in chain {
        if let Some(o) = rules.get(level).map(FolderRules::list_overrides) {
            settings.sort = o.sort.unwrap_or(settings.sort);
            settings.group_by_date = o.group_by_date.unwrap_or(settings.group_by_date);
            settings.include_subfolders = o.include_subfolders.unwrap_or(settings.include_subfolders);
        }
    }
    settings
}
