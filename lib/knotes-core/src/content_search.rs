use crate::{BoxError, Span};

/// A block that matched a content query. Text is `note_text(path)[span]`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockHit {
    pub path: String,
    /// Position of the block in its note.
    pub ord: u32,
    pub span: Span,
    /// Text of the heading of the block's section.
    pub heading: Option<String>,
    /// Lower is better (bm25).
    pub score: f64,
}

/// Ranked full-text search over note blocks. The query is plain user text, never syntax:
/// every word must occur in the block.
pub trait ContentSearch {
    fn search_blocks(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>, BoxError>;
}
