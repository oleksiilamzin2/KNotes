use crate::{EmbedError, Passage, Vector};

/// Turns text into vectors, one for each passage, in order.
pub trait Embedder: Send + Sync {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError>;
}
