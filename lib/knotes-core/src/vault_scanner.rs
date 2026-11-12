use crate::VaultFile;

/// Lists every visible file of a vault, sorted by path.
pub trait VaultScanner {
    fn scan(&self) -> std::io::Result<Vec<VaultFile>>;
}
