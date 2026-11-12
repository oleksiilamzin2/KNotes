use std::time::Duration;

/// Tells that files in the vault changed. Consumers re-sync the index; the paths also tell
/// the UI which open notes changed on disk.
pub trait ChangeFeed {
    /// The next batch of changed vault-relative paths (sorted, deduplicated), or `None` if
    /// nothing changed within `timeout`.
    fn next_change(&self, timeout: Duration) -> Option<Vec<String>>;
}
