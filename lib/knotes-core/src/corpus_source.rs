use std::sync::Arc;

use crate::{BoxError, TextCorpus};

/// Gives a snapshot of all note texts for scanning. A snapshot stays valid and unchanged
/// after later writes, so it can be scanned on another thread.
pub trait CorpusSource {
    type Corpus: TextCorpus + Send + 'static;
    /// Notes in path order.
    fn corpus(&self) -> Result<Arc<Self::Corpus>, BoxError>;
}
