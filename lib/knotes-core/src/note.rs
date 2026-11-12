/// Byte range in the note source. `u32` keeps index rows small; notes above 4 GiB are not supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start: start as u32, end: end as u32 }
    }
}

/// A Markdown heading: `level` 1..=6, `text` without markup, `span` covers the whole heading line(s).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub span: Span,
}

/// Everything the indexer extracts from one note.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedNote {
    pub headings: Vec<Heading>,
}
