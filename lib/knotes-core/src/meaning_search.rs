use crate::{BlockHit, BoxError};

/// Blocks near a query in meaning, whatever its words (ADR 0028).
pub trait MeaningSearch {
    /// The `limit` nearest, nearest first (`score` lower the nearer, as full-text search's is),
    /// among the documents of `scope` only when given (the list a search is made in).
    fn nearest(&self, query: &str, limit: usize, scope: Option<&[String]>) -> Result<Vec<BlockHit>, BoxError>;
}
