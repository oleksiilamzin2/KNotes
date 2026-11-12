/// Moves or renames note files inside a vault.
pub trait NoteMover {
    /// Moves the file at `from` to `to` (both vault-relative), creating folders as needed.
    /// Fails with `AlreadyExists` instead of replacing another file.
    fn move_note(&self, from: &str, to: &str) -> std::io::Result<()>;
}
