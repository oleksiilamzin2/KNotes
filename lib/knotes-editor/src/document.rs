//! An open note as the editor sees it: the text last saved or loaded, and whether there are
//! unsaved edits (ADR 0011).

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
}

impl Document {
    pub fn open(text: &str) -> Self {
        Self { saved: text.to_owned(), dirty: false }
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
