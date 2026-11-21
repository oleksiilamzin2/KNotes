use crate::BoxError;

/// Adds files to the vault's attachments folder (ADR 0020).
pub trait AttachmentStore {
    /// Writes `bytes` as a new file named `name` in the attachments folder (`name 1`, `name 2`…
    /// when it is taken; no file is replaced) and returns its vault path.
    fn add_attachment(&self, name: &str, bytes: &[u8]) -> std::io::Result<String>;
}

/// A vault file that is no note and no canvas (an image, a PDF, any other), and how many
/// links point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub path: String,
    pub size: u64,
    pub links: u32,
}

/// A link to an image or a PDF that is not in the vault: its target as written, and the note
/// it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAttachment {
    pub target: String,
    pub source: String,
}

/// The vault's attachments as the index knows them.
pub trait AttachmentIndex {
    /// Every attachment, by path.
    fn attachments(&self) -> Result<Vec<Attachment>, BoxError>;
    /// Every link to an image or a PDF that is not there, by target then note; one of a note
    /// to the same target once.
    fn missing_attachments(&self) -> Result<Vec<MissingAttachment>, BoxError>;
}
