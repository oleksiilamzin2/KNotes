use crate::{BoxError, Link};

/// An outgoing link and the vault file it resolves to (`None`: broken or external).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutLink {
    pub link: Link,
    pub resolved: Option<String>,
}

/// A link from another note (`source`) to the note asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackLink {
    pub source: String,
    pub link: Link,
}

/// Links between vault files, resolved as Obsidian does (ADR 0009).
pub trait LinkGraph {
    /// Outgoing links of a note, in source order.
    fn links_from(&self, path: &str) -> Result<Vec<OutLink>, BoxError>;
    /// Links from other notes to `path`, by source path then position; links of a note to
    /// itself are not included.
    fn backlinks(&self, path: &str) -> Result<Vec<BackLink>, BoxError>;
}
