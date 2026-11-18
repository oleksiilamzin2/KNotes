//! An open vault: lock, background indexing and snapshots for the window (ADR 0010).

mod app_settings;
mod clock;
mod lock;
mod memory;
mod paths;
mod session;
mod snapshot;

pub use app_settings::{AppSettings, SETTINGS_FILE};
pub use clock::now;
pub use lock::VaultLock;
pub use paths::{app_data_dir, embedder_files, index_dir};
pub use session::{Event, VaultSession};
pub use snapshot::Snapshot;
