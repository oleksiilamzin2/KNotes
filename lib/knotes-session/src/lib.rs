//! An open vault: lock, background indexing and snapshots for the window (ADR 0010).

mod app_settings;
mod lock;
mod memory;
mod paths;
mod session;
mod snapshot;

pub use app_settings::{AppSettings, SETTINGS_FILE};
pub use lock::VaultLock;
pub use paths::{app_data_dir, index_dir};
pub use session::{Event, VaultSession};
pub use snapshot::Snapshot;
