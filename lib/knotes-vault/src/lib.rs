//! File system access to a vault.

mod fs_reviews;
mod fs_trash;
mod fs_vault;
mod fs_watcher;

pub use fs_vault::{FsVault, start_vault};
pub use fs_watcher::{FsWatcher, RESCAN};
