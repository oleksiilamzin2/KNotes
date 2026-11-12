//! SQLite index of a vault (WAL mode, one writer, many readers).

mod codec;
mod content_pack;
mod corpus;
mod fts_query;
mod light_stemmer;
mod link_resolver;
mod link_targets;
mod name_key;
mod read;
mod schema;
mod search;
mod sqlite_index;
mod stemmer;
mod write;

pub use corpus::TextSnapshot;
pub use link_targets::LinkTargets;
pub use sqlite_index::{MaintenanceReport, SqliteIndex};
