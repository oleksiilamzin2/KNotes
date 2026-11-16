/// A deleted file or folder, kept to be restored (ADR 0032).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    pub id: String,
    /// Where it was in the vault.
    pub path: String,
    pub folder: bool,
    /// When it was deleted, Unix milliseconds.
    pub deleted_ms: i64,
    /// The notes (Markdown files) it holds: 1 for a note, those inside for a folder.
    pub notes: usize,
}

/// The vault's recently deleted files and folders, kept to be restored (ADR 0032).
pub trait TrashStore {
    /// Moves the file or folder at `path` into the trash, with the rules files of the folders
    /// above it, so that a restore brings back what they said.
    fn trash(&self, path: &str, now_ms: i64) -> std::io::Result<TrashEntry>;
    /// Every entry, the newest first.
    fn trashed(&self) -> std::io::Result<Vec<TrashEntry>>;
    /// Moves the entry back where it was (the folders above made again with their rules,
    /// another name beside it when the place is taken); gives the path it has now.
    fn restore(&self, id: &str) -> std::io::Result<String>;
    /// Deletes the entry for good.
    fn purge(&self, id: &str) -> std::io::Result<()>;
}
