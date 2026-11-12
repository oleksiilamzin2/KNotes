use crate::{BoxError, ParsedNote, VaultFile};

/// One change to the index. Markdown notes come with their source and parse result;
/// other files (canvases, attachments) only with their metadata.
pub enum IndexChange<'a> {
    Upsert { file: &'a VaultFile, note: Option<(&'a str, &'a ParsedNote)> },
    Remove { path: &'a str },
}

/// Applies a batch of changes atomically (one transaction per call).
pub trait IndexWriter {
    fn apply(&mut self, changes: &[IndexChange<'_>]) -> Result<(), BoxError>;
}
