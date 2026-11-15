use crate::{BlockHit, BoxError};

/// Blocks near a query in meaning, whatever its words (ADR 0028).
pub trait MeaningSearch {
    /// The `limit` nearest, nearest first (`score` lower the nearer, as full-text search's is),
    /// among the documents of `scope` only when given (the list a search is made in).
    fn nearest(&self, query: &str, limit: usize, scope: Option<&[String]>) -> Result<Vec<BlockHit>, BoxError>;

    /// The `limit` documents nearest the document at `path` in meaning (all its blocks
    /// together), each once as its nearest block, nearest first, scored as `nearest` scores; the
    /// document itself never, none when it has no vectors yet.
    fn near_document(&self, path: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError>;
}
