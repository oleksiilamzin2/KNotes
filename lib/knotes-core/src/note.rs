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

/// Unit of full-text search and of reading by the chat agent: a heading section,
/// or a piece of one when the section is too long. Text is `source[span]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub span: Span,
    /// Index into `ParsedNote::headings` of the section this block belongs to.
    pub heading: Option<u32>,
}

/// Obsidian block identifier `^name` at the end of a line; target of `[[note#^name]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAnchor {
    pub name: String,
    /// Covers `^name`.
    pub span: Span,
}

/// YAML frontmatter at the top of a note. Invalid YAML gives empty values, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frontmatter {
    /// The whole block, `---` lines included.
    pub span: Span,
    /// From `tags` / `tag`, without `#`.
    pub tags: Vec<String>,
    /// From `aliases` / `alias`.
    pub aliases: Vec<String>,
}

/// Everything the indexer extracts from one note.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedNote {
    pub frontmatter: Option<Frontmatter>,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    /// Inline tags only; frontmatter tags live in `frontmatter.tags`.
    pub tags: Vec<Tag>,
    pub anchors: Vec<BlockAnchor>,
    /// In source order; frontmatter is excluded.
    pub blocks: Vec<Block>,
    /// Plain-text start of the note for the note list (no title, frontmatter or code).
    pub snippet: String,
}
