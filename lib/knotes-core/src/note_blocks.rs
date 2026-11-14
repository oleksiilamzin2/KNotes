use crate::{BlockHit, BoxError};

/// The search blocks of an indexed note or PDF, as the full-text search holds them: what search
/// by meaning embeds (ADR 0028), so its finds and the full-text ones name the same blocks.
pub trait NoteBlocks {
    /// In order; empty for an unknown path. Their `score` is 0.
    fn note_blocks(&self, path: &str) -> Result<Vec<BlockHit>, BoxError>;
}
