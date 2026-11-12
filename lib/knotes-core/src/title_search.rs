/// A file whose title (or alias, or path) matched a title query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleHit {
    pub path: String,
    /// Set when the best match was an alias rather than the title or path.
    pub alias: Option<String>,
    /// Higher is better.
    pub score: u32,
}

/// Fuzzy search over titles, aliases and paths, fzf-style (`'exact`, `^prefix`, `suffix$`, `!not`).
pub trait TitleSearch {
    /// Best first, one hit per path.
    fn search_titles(&self, query: &str, limit: usize) -> Vec<TitleHit>;
}
