use serde_json::{Map, Value, json};
use unicode_normalization::UnicodeNormalization;

use crate::{FolderSpace, SHARED, Space, SpaceColor};

/// A folder's rules file with its space set as `space` says, its other keys kept.
pub fn set_folder_space(rules: &str, space: FolderSpace) -> Result<String, String> {
    let mut root = object(rules)?;
    match space {
        FolderSpace::Inherit => root.remove("space"),
        FolderSpace::None => root.insert("space".into(), Value::Null),
        FolderSpace::In(id) => root.insert("space".into(), json!(id)),
        FolderSpace::Shared => root.insert("space".into(), json!(SHARED)),
    };
    Ok(pretty(&root))
}

/// A folder's rules file with its own `ai` set (`None`: the key goes, the folder takes its
/// parent's), its other keys kept (ADR 0036).
pub fn set_folder_ai(rules: &str, ai: Option<bool>) -> Result<String, String> {
    let mut root = object(rules)?;
    match ai {
        Some(ai) => root.insert("ai".into(), json!(ai)),
        None => root.remove("ai"),
    };
    Ok(pretty(&root))
}

/// A folder's rules file with the file `name`'s own `ai` set under `files` (`None`: its entry
/// goes, and `files` with its last entry), its other keys kept (ADR 0036).
pub fn set_file_ai(rules: &str, name: &str, ai: Option<bool>) -> Result<String, String> {
    let mut root = object(rules)?;
    let mut files = root.remove("files").and_then(|f| if let Value::Object(f) = f { Some(f) } else { None }).unwrap_or_default();
    // The name as written there, composed or not (macOS writes some names decomposed).
    let written = files.keys().find(|key| same_name(key, name)).cloned();
    let mut entry = written.and_then(|key| files.remove(&key)).and_then(|e| if let Value::Object(e) = e { Some(e) } else { None }).unwrap_or_default();
    match ai {
        Some(ai) => {
            entry.insert("ai".into(), json!(ai));
        }
        None => {
            entry.remove("ai");
        }
    }
    if !entry.is_empty() {
        files.insert(name.to_owned(), Value::Object(entry));
    }
    if !files.is_empty() {
        root.insert("files".into(), Value::Object(files));
    }
    Ok(pretty(&root))
}

/// The file `name`'s own `ai` in a folder's rules file (ADR 0036), if it says one; none when
/// the file does not parse.
pub fn file_ai(rules: &str, name: &str) -> Option<bool> {
    let root = object(rules).ok()?;
    let (_, entry) = root.get("files")?.as_object()?.iter().find(|(key, _)| same_name(key, name))?;
    entry.get("ai")?.as_bool()
}

fn same_name(a: &str, b: &str) -> bool {
    a.nfc().eq(b.nfc())
}

/// The root's rules file listing `spaces`, in order, its other keys kept (none: no key).
pub fn set_spaces(rules: &str, spaces: &[Space]) -> Result<String, String> {
    let mut root = object(rules)?;
    if spaces.is_empty() {
        root.remove("spaces");
    } else {
        let listed = spaces.iter().map(|s| json!({"id": s.id, "name": s.name, "color": s.color.name()})).collect();
        root.insert("spaces".into(), Value::Array(listed));
    }
    Ok(pretty(&root))
}

/// A new space named `name`, after `spaces`: an id from its name that no other has, and the
/// first colour no space has (round the palette again when all are taken).
pub fn new_space(spaces: &[Space], name: &str) -> Space {
    let color = SpaceColor::ALL.into_iter().find(|color| spaces.iter().all(|space| space.color != *color)).unwrap_or(SpaceColor::ALL[spaces.len() % SpaceColor::ALL.len()]);
    Space { id: new_space_id(spaces, name), name: name.to_owned(), color }
}

/// An id for a new space named `name`: its letters in Latin (Ukrainian transliterated), lower
/// case, words joined by dashes; numbered when another space has it; "space" when the name has
/// no letter to spell it with.
pub fn new_space_id(spaces: &[Space], name: &str) -> String {
    let mut spelled = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            spelled.push(c);
        } else if let Some(latin) = latin(c) {
            spelled.push_str(latin);
        } else if !spelled.ends_with('-') && !spelled.is_empty() && !matches!(c, '\'' | '’' | 'ʼ') {
            spelled.push('-');
        }
    }
    let base = spelled.trim_matches('-');
    let base = if base.is_empty() { "space" } else { base };
    let taken = |id: &str| spaces.iter().any(|s| s.id == id);
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken(id)).expect("some number is free")
}

/// A Cyrillic letter in Latin (Ukrainian's official transliteration, Russian's extra letters).
fn latin(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "h",
        'ґ' => "g",
        'д' => "d",
        'е' | 'э' | 'ё' => "e",
        'є' => "ie",
        'ж' => "zh",
        'з' => "z",
        'и' | 'ы' => "y",
        'і' | 'ї' | 'й' => "i",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ь' | 'ъ' => "",
        'ю' => "iu",
        'я' => "ia",
        _ => return None,
    })
}

/// The rules as a JSON object; none yet when empty.
fn object(rules: &str) -> Result<Map<String, Value>, String> {
    if rules.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(rules) {
        Ok(Value::Object(root)) => Ok(root),
        Ok(_) => Err("the rules file must hold a JSON object".into()),
        Err(e) => Err(format!("the rules file: {e}")),
    }
}

/// Pretty JSON with a trailing newline, as the app writes rules files.
fn pretty(root: &Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(root).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}
