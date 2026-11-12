/// Writes note files of a vault.
pub trait NoteWriter {
    /// Replaces (or creates) the file at a vault-relative path atomically: readers and
    /// other devices never see half a note.
    fn write_note(&self, path: &str, text: &str) -> std::io::Result<()>;
}
