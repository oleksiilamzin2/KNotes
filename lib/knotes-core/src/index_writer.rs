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

    /// Deferred housekeeping (merging index segments, checkpoints), for idle time: keeping it
    /// out of `apply` keeps a single-note update fast. Safe to call at any time.
    fn maintain(&mut self) -> Result<(), BoxError> {
        Ok(())
    }
}
