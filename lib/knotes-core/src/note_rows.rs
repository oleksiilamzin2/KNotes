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
}
