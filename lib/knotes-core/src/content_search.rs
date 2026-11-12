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

/// How near each other the words of a content query must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Within {
    /// Anywhere in the note (Obsidian's `content:`); one hit per note, its best block.
    Note,
    /// In one block (Obsidian's `block:`).
    Block,
}

/// A content query. The text is plain user text, never syntax: every word must occur.
#[derive(Debug, Clone, Copy)]
pub struct ContentQuery<'a> {
    pub text: &'a str,
    pub within: Within,
    /// Search as you type: every word also matches as the start of a word, however short
    /// (`люц ба` finds "Люцифер" and "Баптист"); otherwise words of up to three letters match
    /// whole words only.
    pub prefixes: bool,
    /// Only these note paths (unknown paths are ignored); `None`: all notes.
    pub scope: Option<&'a [String]>,
    pub limit: usize,
}

/// Ranked full-text search over note blocks.
pub trait ContentSearch {
    /// Best first.
    fn search(&self, query: &ContentQuery) -> Result<Vec<BlockHit>, BoxError>;

    /// Blocks holding every word, best first.
    fn search_blocks(&self, query: &str, scope: Option<&[String]>, limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        self.search(&ContentQuery { text: query, within: Within::Block, prefixes: false, scope, limit })
    }
}
