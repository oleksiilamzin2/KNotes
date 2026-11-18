//! List search in a background thread with its own read-only index connection (ADR 0015):
//! typing in the search field never waits for the title matcher, full-text search or search by
//! meaning (ADR 0028), which reads the vectors Knotes keeps beside the index.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

pub use knotes_app::{SearchRequest, SearchResponse};
use knotes_app::SearchMode;
use knotes_core::MeaningSearch;
use knotes_embed::Embedder;
use knotes_index::SqliteIndex;
use knotes_vectors::{MeaningIndex, VectorStore};

pub struct SearchWorker {
    requests: Sender<SearchRequest>,
}

impl SearchWorker {
    /// Starts the thread; `deliver` runs on it with each answer. The thread ends when the
    /// worker is dropped.
    pub fn start(index_dir: PathBuf, deliver: impl Fn(SearchResponse) + Send + 'static) -> Self {
        let (requests, incoming) = channel::<SearchRequest>();
        std::thread::Builder::new()
            .name("knotes-search".into())
            .spawn(move || {
                let mut reader: Option<SqliteIndex> = None;
                // Search by meaning over the chosen model's vectors, and the stores' generation it
                // was made in (another model chosen, or a store cleared, makes it again).
                let mut meaning: Option<(u64, MeaningIndex)> = None;
                while let Ok(mut request) = incoming.recv() {
                    // Queries typed meanwhile replace this one: only the latest matters.
                    while let Ok(newer) = incoming.try_recv() {
                        request = newer;
                    }
                    if reader.is_none() {
                        reader = SqliteIndex::open_reader(&index_dir).ok();
                    }
                    // Made at the first search by meaning, and again in a later generation.
                    let generation = crate::meaning::generation();
                    if request.mode == SearchMode::Meaning && meaning.as_ref().is_none_or(|(made_in, _)| *made_in != generation) {
                        meaning = crate::meaning::model().zip(crate::meaning::embedder()).and_then(|(model, embedder)| {
                            VectorStore::open_reader(&index_dir, &model.store_file()).ok().map(|store| (generation, MeaningIndex::over_store(embedder as Arc<dyn Embedder>, store)))
                        });
                    }
                    let meaning = meaning.as_ref().filter(|(made_in, _)| *made_in == generation).map(|(_, m)| m as &dyn MeaningSearch);
                    deliver(knotes_app::search(&request, reader.as_ref(), meaning));
                }
            })
            .expect("search thread starts");
        Self { requests }
    }

    pub fn request(&self, request: SearchRequest) {
        let _ = self.requests.send(request);
    }
}

/// The answer to `request` by its names and text, found at once on the caller's thread (probe
/// mode, where a window's search is driven and read in one go); search by meaning gives none.
pub fn search_now(request: &SearchRequest, reader: Option<&SqliteIndex>) -> SearchResponse {
    knotes_app::search(request, reader, None)
}
