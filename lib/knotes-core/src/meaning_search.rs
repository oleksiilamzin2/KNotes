use crate::{BlockHit, BoxError};

/// Blocks near a query in meaning, whatever its words (ADR 0028).
pub trait MeaningSearch {
    /// The `limit` nearest, nearest first (`score` lower the nearer, as full-text search's is).
    fn nearest(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError>;
}
