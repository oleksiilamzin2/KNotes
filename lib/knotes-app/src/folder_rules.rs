//! Per-folder view rules in a `.knotes` JSON file inside the folder (ADR 0010).
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

use crate::{ListSettings, SortOrder};

pub const RULES_FILE: &str = ".knotes";

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
        let list = self.root.entry("list").or_insert_with(|| json!({}));
        if !list.is_object() {
            *list = json!({});
        }
        let list = list.as_object_mut().expect("object");
        let sort = match settings.sort {
            SortOrder::Modified => "modified",
            SortOrder::Title => "title",
        };
        list.insert("sort".into(), json!(sort));
        list.insert("groupByDate".into(), json!(settings.group_by_date));
        list.insert("includeSubfolders".into(), json!(settings.include_subfolders));
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

/// List settings of `folder` (`""` is the vault root), given the rules of the folders that
/// have a `.knotes` file.
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
