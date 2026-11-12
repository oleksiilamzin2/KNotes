//! Keeps the index in sync with the vault.

mod indexer;
mod plan;

pub use indexer::{Indexer, SyncReport};
pub use plan::{Plan, plan};
