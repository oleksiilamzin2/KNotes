use crate::{BoxError, Link};

/// Links between notes.
pub trait LinkGraph {
    /// Outgoing links of a note, in source order.
    fn links_from(&self, path: &str) -> Result<Vec<Link>, BoxError>;
}
