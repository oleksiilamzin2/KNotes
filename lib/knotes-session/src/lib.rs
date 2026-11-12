//! An open vault: lock, background indexing and snapshots for the window (ADR 0010).

mod lock;
mod paths;
mod session;
mod snapshot;

pub use lock::VaultLock;
pub use paths::index_dir;
pub use session::{Event, VaultSession};
pub use snapshot::Snapshot;
