//! The embedding model's helper, one for the whole app (ADR 0028): the chat's search, the list's
//! search by meaning and the thread keeping the vectors share it.

use std::sync::{Arc, OnceLock};

use knotes_embed::ProcessEmbedder;

static EMBEDDER: OnceLock<Option<Arc<ProcessEmbedder>>> = OnceLock::new();

/// The helper; none in the probe (its windows never start the model) or without the helper and
/// its model beside the app.
pub fn embedder() -> Option<Arc<ProcessEmbedder>> {
    EMBEDDER
        .get_or_init(|| {
            if crate::probe::enabled() {
                return None;
            }
            std::env::current_exe().ok().and_then(|exe| knotes_session::embedder_files(&exe)).map(|(helper, model)| Arc::new(ProcessEmbedder::new(helper, model)))
        })
        .clone()
}
