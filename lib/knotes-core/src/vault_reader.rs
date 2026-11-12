/// Reads note files of a vault.
pub trait VaultReader {
    /// Text of the file at a vault-relative path; invalid UTF-8 is replaced, not an error.
    fn read_text(&self, path: &str) -> std::io::Result<String>;
}
