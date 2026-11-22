//! The starred folders and files: kept in the vault root's rules file as
//! `"stars": ["Projects/", "Projects/Plan.md"]` (a folder's path ends with `/`), in the order
//! they were starred, so they travel with the vault.

use serde_json::{Value, json};

use crate::{FolderRules, Moves};

/// The star of the folder at `folder`.
pub fn folder_star(folder: &str) -> String {
    format!("{}/", folder.trim_matches('/'))
}

/// The folder a star marks; none for a file's.
pub fn starred_folder(star: &str) -> Option<&str> {
    star.strip_suffix('/')
}

impl FolderRules {
    /// The stars this (root) file lists, each once.
    pub fn stars(&self) -> Vec<String> {
        let mut stars: Vec<String> = Vec::new();
        for star in self.object().get("stars").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !star.trim_matches('/').is_empty() && !stars.iter().any(|s| s == star) {
                stars.push(star.to_owned());
            }
        }
        stars
    }
}

/// The root rules file's `text` with `stars` in it, its other keys kept; the key is taken out
/// with no stars left. An empty text is a file not there yet.
pub fn with_stars(text: &str, stars: &[String]) -> Result<String, String> {
    let mut root = match FolderRules::parse(if text.trim().is_empty() { "{}" } else { text })?.object().clone() {
        root if stars.is_empty() && !root.contains_key("stars") => return Ok(text.to_owned()),
        root => root,
    };
    if stars.is_empty() {
        root.remove("stars");
    } else {
        root.insert("stars".into(), json!(stars));
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// `stars` with `star` taken out, or added last when it was not there.
pub fn toggled_star(mut stars: Vec<String>, star: &str) -> Vec<String> {
    match stars.iter().position(|s| s == star) {
        Some(at) => {
            stars.remove(at);
        }
        None => stars.push(star.to_owned()),
    }
    stars
}

/// `stars` after files moved (`moves`: new path by old) or a folder did (`folder`: from, to):
/// a star follows its file, its folder, and the folder it is inside.
pub fn moved_stars(stars: &[String], moves: &Moves, folder: Option<(&str, &str)>) -> Vec<String> {
    stars
        .iter()
        .map(|star| {
            if let Some(to) = moves.get(star) {
                return to.clone();
            }
            match folder {
                Some((from, to)) => match star.strip_prefix(from).filter(|rest| rest.starts_with('/')) {
                    Some(rest) => format!("{to}{rest}"),
                    None => star.clone(),
                },
                None => star.clone(),
            }
        })
        .collect()
}
