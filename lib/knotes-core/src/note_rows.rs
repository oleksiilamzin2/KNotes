use crate::BoxError;

/// What the note list shows for a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRow {
    pub path: String,
    pub mtime_ns: i64,
    pub snippet: String,
}

pub trait NoteRows {
    /// All Markdown notes and PDFs (the search finds PDFs too, ADR 0022), sorted by path.
    fn note_rows(&self) -> Result<Vec<NoteRow>, BoxError>;

    /// When the file at `path` last changed (nanoseconds since 1970), if it is indexed.
    fn modified(&self, path: &str) -> Result<Option<i64>, BoxError>;
}
