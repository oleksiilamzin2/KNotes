use crate::BoxError;

/// The indexed source text of a note, as it was when last indexed.
pub trait NoteText {
    /// `None` for unknown paths and for files that are not Markdown notes.
    fn note_text(&self, path: &str) -> Result<Option<String>, BoxError>;
}
