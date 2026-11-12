use crate::{BoxError, Span};

/// Texts to scan: the notes of the vault, or of a scope. `Sync` so scans can run in parallel.
pub trait TextCorpus: Sync {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Path and UTF-8 text of note `i`.
    fn note(&self, i: usize) -> (&str, &[u8]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseMode {
    /// Case-insensitive unless the pattern has an uppercase letter (as in ripgrep).
    #[default]
    Smart,
    Sensitive,
    Insensitive,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanQuery<'a> {
    pub pattern: &'a str,
    /// `false`: the pattern is a literal string.
    pub regex: bool,
    pub case: CaseMode,
}

/// One match: `span` is the byte range of the match in the note, `line` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    pub path: String,
    pub line: u32,
    pub span: Span,
    pub line_text: String,
}

/// Exact or regex search through note texts, like ripgrep.
pub trait ContentScan {
    /// Hits in corpus order, then position order; at most `limit`.
    fn scan(&self, corpus: &impl TextCorpus, query: &ScanQuery, limit: usize) -> Result<Vec<ScanHit>, BoxError>;
}
