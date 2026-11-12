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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// `[[target]]`, always points inside the vault.
    Wiki,
    /// `[text](path)` without a URL scheme, points inside the vault.
    Markdown,
    /// Any link with a URL scheme (`https:`, `mailto:`, `obsidian:` ...).
    External,
}

/// A link or an embed (`embed == true` for `![[...]]` and `![](...)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    pub embed: bool,
    /// Path part as written (percent-decoded for Markdown links). Empty for `[[#Heading]]`.
    /// For external links: the full URL.
    pub target: String,
    /// Heading (`Heading`) or block (`^id`) after `#`, without the `#`.
    pub subpath: Option<String>,
    /// Alias, link text or embed size; `None` when the link shows its target.
    pub display: Option<String>,
    pub span: Span,
}

/// Inline tag `#name` or `#area/sub`; `name` is as written, without `#`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub span: Span,
}

/// Everything the indexer extracts from one note.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedNote {
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    pub tags: Vec<Tag>,
}
