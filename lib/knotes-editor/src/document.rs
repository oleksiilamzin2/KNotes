//! An open note as the editor sees it: the text last saved or loaded, whether there are
//! unsaved edits, and the name its title gave it (ADR 0011, 0019).

use crate::title::title_words;

/// What to do when the note's file changed on disk.
#[derive(Debug, PartialEq, Eq)]
pub enum DiskChange {
    /// The file holds what the editor saved or shows: nothing to do.
    Ignore,
    /// Changed outside and the editor has no unsaved edits: show the new text.
    Reload,
    /// Changed outside while the editor has unsaved edits: keep both versions.
    Conflict,
}

#[derive(Debug)]
pub struct Document {
    /// Text last written by the editor or read from disk.
    saved: String,
    dirty: bool,
    /// Created by "new note" and not named after its first line yet.
    untitled: bool,
    /// The title's words when the note was opened or last named (ADR 0019).
    name: Option<String>,
}

impl Document {
    pub fn open(text: &str) -> Self {
        Self { saved: text.to_owned(), dirty: false, untitled: false, name: title_words(text).map(str::to_owned) }
    }

    /// A new, empty note whose file takes the name of its first line (ADR 0011).
    pub fn untitled() -> Self {
        Self { untitled: true, ..Self::open("") }
    }

    /// Whether to name the file after the title now. The title is not empty and differs from
    /// the name the note had, and it is `finished`: the note is being left, or Return ended
    /// the title. A new note's title is also finished by any line break after it. Naming
    /// mid-line would give a name cut where the user paused (ADR 0011, 0019).
    pub fn wants_name(&self, text: &str, finished: bool) -> bool {
        if !finished && !(self.untitled && text.contains('\n')) {
            return false;
        }
        title_words(text).is_some_and(|title| !title.trim().is_empty() && self.name.as_deref() != Some(title))
    }

    /// The file was named after the title of `text` (or its renaming was declined): the next
    /// rename waits for the title to change again.
    pub fn named(&mut self, text: &str) {
        self.untitled = false;
        self.name = title_words(text).map(str::to_owned);
    }

    pub fn edited(&mut self) {
        self.dirty = true;
    }

    /// The editor wrote `text` to the file.
    pub fn saved(&mut self, text: &str) {
        self.saved = text.to_owned();
        self.dirty = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The file now holds `disk`; the editor shows `editor`. A change counts as outside only
    /// when `disk` differs from what the editor last saved, so its own writes never reload.
    pub fn on_disk(&mut self, disk: &str, editor: &str) -> DiskChange {
        if disk == editor {
            self.saved(disk);
            DiskChange::Ignore
        } else if disk == self.saved {
            DiskChange::Ignore
        } else if !self.dirty {
            self.saved = disk.to_owned();
            // A title changed elsewhere is the note's name now, not a rename to make here.
            self.name = title_words(disk).map(str::to_owned);
            DiskChange::Reload
        } else {
            DiskChange::Conflict
        }
    }
}

/// `Folder/Name (конфлікт <stamp>).md` next to `Folder/Name.md`.
pub fn conflict_path(path: &str, stamp: &str) -> String {
    let (stem, ext) = path.rsplit_once('.').filter(|(_, e)| !e.contains('/')).unwrap_or((path, ""));
    let ext = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    format!("{stem} (конфлікт {stamp}){ext}")
}
