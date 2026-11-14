//! The vault's blocks as vectors and the search by meaning over them (ADR 0028): what text a
//! block is embedded as, the vectors held in memory and searched by brute force, and the
//! search itself, a query embedded and its nearest blocks found.

mod embed_notes;
mod meaning_index;
mod passage_of;
mod snapshot;
mod update;
mod vector_store;

pub use embed_notes::embed_notes;
pub use meaning_index::MeaningIndex;
pub use passage_of::block_passage;
pub use snapshot::Snapshot;
pub use update::update;
pub use vector_store::{VECTORS_FILE, VectorStore};

/// How the vectors are made, kept with them: another model, width or text for a block, and the
/// store is embedded again.
pub const EMBEDDING: &str = "embeddinggemma-300M-Q8_0, first 256, title and heading › block, v1";
