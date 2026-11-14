use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use knotes_core::{BlockHit, BoxError, MeaningSearch};
use knotes_embed::{Embedder, Passage};

use crate::{Snapshot, VectorStore};

/// How often the store is checked for vectors written since it was read.
const LOOK_AGAIN: Duration = Duration::from_secs(10);

/// Search by meaning: the query embedded, its nearest blocks found in the snapshot, which is
/// read again from the store (when there is one) once the store has changed.
pub struct MeaningIndex {
    embedder: Arc<dyn Embedder>,
    snapshot: RwLock<Snapshot>,
    store: Option<Mutex<Watched>>,
}

struct Watched {
    store: VectorStore,
    /// The store's version when last read; none before the first read.
    version: Option<i64>,
    checked: Instant,
}

impl MeaningIndex {
    /// Over vectors given once.
    pub fn new(embedder: Arc<dyn Embedder>, snapshot: Snapshot) -> Self {
        Self { embedder, snapshot: RwLock::new(snapshot), store: None }
    }

    /// Over the vectors in `store`, as they are written.
    pub fn over_store(embedder: Arc<dyn Embedder>, store: VectorStore) -> Self {
        let watched = Watched { store, version: None, checked: Instant::now() };
        Self { embedder, snapshot: RwLock::new(Snapshot::default()), store: Some(Mutex::new(watched)) }
    }

    fn refresh(&self) -> Result<(), BoxError> {
        let Some(watched) = &self.store else { return Ok(()) };
        let mut watched = watched.lock().map_err(|_| "the store's lock was poisoned")?;
        if watched.version.is_some() && watched.checked.elapsed() < LOOK_AGAIN {
            return Ok(());
        }
        watched.checked = Instant::now();
        let version = watched.store.version()?;
        if watched.version != Some(version) {
            let snapshot = watched.store.snapshot()?;
            *self.snapshot.write().map_err(|_| "the vectors' lock was poisoned")? = snapshot;
            watched.version = Some(version);
        }
        Ok(())
    }
}

impl MeaningSearch for MeaningIndex {
    fn nearest(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        self.refresh()?;
        let snapshot = self.snapshot.read().map_err(|_| "the vectors' lock was poisoned")?;
        if snapshot.is_empty() {
            return Ok(Vec::new());
        }
        let vector = self.embedder.embed(&[Passage::Query(query.to_string())])?.remove(0);
        Ok(snapshot.nearest(&vector, limit))
    }
}
