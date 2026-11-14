use std::sync::{Arc, RwLock};

use knotes_core::{BlockHit, BoxError, MeaningSearch};
use knotes_embed::{Embedder, Passage};

use crate::Snapshot;

/// Search by meaning: the query embedded, its nearest blocks found in the snapshot.
pub struct MeaningIndex {
    embedder: Arc<dyn Embedder>,
    snapshot: RwLock<Snapshot>,
}

impl MeaningIndex {
    pub fn new(embedder: Arc<dyn Embedder>, snapshot: Snapshot) -> Self {
        Self { embedder, snapshot: RwLock::new(snapshot) }
    }
}

impl MeaningSearch for MeaningIndex {
    fn nearest(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        let snapshot = self.snapshot.read().map_err(|_| "the vectors' lock was poisoned")?;
        if snapshot.is_empty() {
            return Ok(Vec::new());
        }
        let vector = self.embedder.embed(&[Passage::Query(query.to_string())])?.remove(0);
        Ok(snapshot.nearest(&vector, limit))
    }
}
