/// Reads and writes the per-folder rules files (`.knotes`) of a vault.
pub trait FolderRulesStore {
    /// `(folder, file text)` for each of `folders` (`""` is the vault root) that has a file.
    fn read_rules(&self, folders: &[&str]) -> std::io::Result<Vec<(String, String)>>;
    /// Replaces the folder's file atomically.
    fn write_rules(&self, folder: &str, text: &str) -> std::io::Result<()>;
}
