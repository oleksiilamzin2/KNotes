/// Adds files to the vault's attachments folder (ADR 0020).
pub trait AttachmentStore {
    /// Writes `bytes` as a new file named `name` in the attachments folder (`name 1`, `name 2`…
    /// when it is taken; no file is replaced) and returns its vault path.
    fn add_attachment(&self, name: &str, bytes: &[u8]) -> std::io::Result<String>;
}
