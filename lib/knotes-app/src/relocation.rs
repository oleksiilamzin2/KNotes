//! Where a note, a PDF or a folder goes when it is renamed or moved (ADR 0019): the new path,
//! or none when nothing would move. Nothing is moved here; `exists` says what the vault holds.

use knotes_core::FileKind;

use crate::{Moves, note_file_stem, unique_file_path, unique_note_path};

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The note or PDF at `old` renamed to `name` in its folder: a number added when the name is
/// taken; a PDF keeps `.pdf` (written or left out). A change of case only is a rename of the
/// same file. None when `name` names nothing, or the file's own name.
pub fn renamed(old: &str, name: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    // Its own path in another case is itself, not a name taken.
    let taken = |path: &str| path.to_lowercase() != old.to_lowercase() && exists(path);
    let new = if FileKind::from_path(old) == FileKind::Pdf {
        let name = name.trim();
        let stem = note_file_stem(if name.to_lowercase().ends_with(".pdf") { &name[..name.len() - 4] } else { name })?;
        unique_file_path(folder_of(old), &format!("{stem}.pdf"), taken)
    } else {
        unique_note_path(folder_of(old), &note_file_stem(name)?, taken)
    };
    (new != old).then_some(new)
}

/// A new note at `old` named after its first line, once it has one (ADR 0011): in its folder,
/// a number added when the name is taken. None while the line names nothing.
pub fn named_after(old: &str, first_line: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    Some(unique_note_path(folder_of(old), &note_file_stem(first_line)?, |path| path != old && exists(path)))
}

/// The file at `path` moved into `folder` (`""`: the vault's root): its name stays, numbered
/// when taken. None when it is there already.
pub fn moved_into(path: &str, folder: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    (folder_of(path) != folder).then(|| unique_file_path(folder, file, exists))
}

/// The folder at `folder` renamed to `name` beside where it is (a `/` in the name becomes
/// `-`). None when the name is empty, its own, or another folder's (a change of case only is
/// a rename of the same folder).
pub fn renamed_folder(folder: &str, name: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let name = name.trim().replace('/', "-");
    if name.is_empty() {
        return None;
    }
    let parent = folder_of(folder);
    let new = if parent.is_empty() { name } else { format!("{parent}/{name}") };
    (new != folder && (new.to_lowercase() == folder.to_lowercase() || !exists(&new))).then_some(new)
}

/// `path` as it is once `folder` is renamed to `new`: the folder itself, or what is in it;
/// none for a path elsewhere.
pub fn in_renamed_folder(path: &str, folder: &str, new: &str) -> Option<String> {
    if path == folder {
        return Some(new.to_owned());
    }
    path.strip_prefix(folder).and_then(|rest| rest.strip_prefix('/')).map(|rest| format!("{new}/{rest}"))
}

/// Every file of `paths` in `folder`, with where it is once the folder is renamed to `new`.
pub fn folder_moves<'a>(paths: impl IntoIterator<Item = &'a str>, folder: &str, new: &str) -> Moves {
    paths.into_iter().filter(|path| *path != folder).filter_map(|path| in_renamed_folder(path, folder, new).map(|to| (path.to_owned(), to))).collect()
}
