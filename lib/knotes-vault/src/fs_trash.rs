//! The vault's recently deleted files and folders (ADR 0032), in `.knotes/trash`: an entry a
//! deletion, a folder named by when it went holding the file or folder as it was (moved, so its
//! text, dates and rules files stay) and `entry.json`: where it was, whether it is a folder,
//! when it went, how many notes it holds, and the rules files of the folders above it.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use knotes_core::{FileKind, TrashEntry, TrashStore};
use serde_json::{Map, Value, json};

use crate::FsVault;

/// Where the entries are, inside the vault.
const TRASH: &str = ".knotes/trash";
const ENTRY: &str = "entry.json";
const RULES: &str = ".knotes.json";

impl TrashStore for FsVault {
    fn trash(&self, path: &str, now_ms: i64) -> io::Result<TrashEntry> {
        let source = self.inside(path)?;
        let folder = fs::symlink_metadata(&source)?.is_dir();
        self.make_knotes_folder()?;
        let trash = self.root.join(TRASH);
        fs::create_dir_all(&trash)?;
        let id = (0..).map(|n| format!("{now_ms}-{n}")).find(|id| !trash.join(id).exists()).expect("a free name");
        let dir = trash.join(&id);
        fs::create_dir(&dir)?;
        // What the folders above it said of themselves, for a restore after they went.
        let mut rules = Map::new();
        let mut at = path;
        while let Some((parent, _)) = at.rsplit_once('/') {
            if let Ok(text) = fs::read_to_string(self.root.join(parent).join(RULES)) {
                rules.insert(parent.to_owned(), json!(text));
            }
            at = parent;
        }
        let notes = if folder { notes_in(&source) } else { usize::from(FileKind::from_path(path) == FileKind::Markdown) };
        let entry = json!({"path": path, "folder": folder, "deleted": now_ms, "notes": notes, "rules": rules});
        fs::write(dir.join(ENTRY), entry.to_string())?;
        if let Err(e) = fs::rename(&source, dir.join(name_of(path))) {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
        Ok(TrashEntry { id, path: path.to_owned(), folder, deleted_ms: now_ms, notes })
    }

    fn trashed_with(&self, known: &[TrashEntry]) -> io::Result<Vec<TrashEntry>> {
        let Ok(dirs) = fs::read_dir(self.root.join(TRASH)) else { return Ok(Vec::new()) };
        let known: HashMap<&str, &TrashEntry> = known.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut entries: Vec<TrashEntry> = dirs
            .filter_map(Result::ok)
            .filter_map(|d| match known.get(d.file_name().to_str()?) {
                Some(entry) => Some((*entry).clone()),
                None => read_entry(&d.path()).map(|(entry, _)| entry),
            })
            .collect();
        entries.sort_by(|a, b| b.deleted_ms.cmp(&a.deleted_ms).then_with(|| b.id.cmp(&a.id)));
        Ok(entries)
    }

    fn restore(&self, id: &str) -> io::Result<String> {
        let dir = self.entry_dir(id)?;
        let (entry, rules) = read_entry(&dir).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no deleted entry {id}")))?;
        // The folders above, made again with their rules where they went.
        let parent = entry.path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let mut at = String::new();
        for segment in parent.split('/').filter(|s| !s.is_empty()) {
            at = if at.is_empty() { segment.to_owned() } else { format!("{at}/{segment}") };
            let folder = self.root.join(&at);
            if !folder.exists() {
                fs::create_dir(&folder)?;
                if let Some(text) = rules.get(&at).and_then(Value::as_str) {
                    fs::write(folder.join(RULES), text)?;
                }
            }
        }
        let target = free_place(&self.root, &entry.path, entry.folder);
        fs::rename(dir.join(name_of(&entry.path)), self.root.join(&target))?;
        fs::remove_dir_all(&dir)?;
        Ok(target)
    }

    fn purge(&self, id: &str) -> io::Result<()> {
        fs::remove_dir_all(self.entry_dir(id)?)
    }

    fn kept_at(&self, id: &str) -> io::Result<PathBuf> {
        let dir = self.entry_dir(id)?;
        let (entry, _) = read_entry(&dir).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no deleted entry {id}")))?;
        Ok(dir.join(name_of(&entry.path)))
    }
}

impl FsVault {
    /// An entry's folder; an id is a time and a number, so nothing outside the trash is taken.
    fn entry_dir(&self, id: &str) -> io::Result<PathBuf> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a deleted entry: {id}")));
        }
        Ok(self.root.join(TRASH).join(id))
    }
}

/// An entry and the rules files it kept, read from its folder.
fn read_entry(dir: &Path) -> Option<(TrashEntry, Map<String, Value>)> {
    let id = dir.file_name()?.to_str()?.to_owned();
    let value: Value = serde_json::from_str(&fs::read_to_string(dir.join(ENTRY)).ok()?).ok()?;
    let entry = TrashEntry {
        id,
        path: value["path"].as_str()?.to_owned(),
        folder: value["folder"].as_bool().unwrap_or(false),
        deleted_ms: value["deleted"].as_i64()?,
        notes: value["notes"].as_u64().unwrap_or(0) as usize,
    };
    Some((entry, value["rules"].as_object().cloned().unwrap_or_default()))
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The notes (Markdown files) below a folder, hidden ones left out.
fn notes_in(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => notes_in(&e.path()),
            Ok(t) if t.is_file() => usize::from(FileKind::from_path(&e.file_name().to_string_lossy()) == FileKind::Markdown),
            _ => 0,
        })
        .sum()
}

/// `path`, or a name beside it numbered from 2 when the place is taken ("Plan 2.md", "Archive 2").
fn free_place(root: &Path, path: &str, folder: bool) -> String {
    if !root.join(path).exists() {
        return path.to_owned();
    }
    let (stem, extension) = match path.rsplit_once('.') {
        Some((stem, extension)) if !folder && !extension.contains('/') && !stem.ends_with('/') => (stem, Some(extension)),
        _ => (path, None),
    };
    (2..)
        .map(|n| match extension {
            Some(extension) => format!("{stem} {n}.{extension}"),
            None => format!("{stem} {n}"),
        })
        .find(|candidate| !root.join(candidate).exists())
        .expect("some number is free")
}
