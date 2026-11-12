//! Search operators as in Obsidian (ADR 0015): `file:` searches note names (the default),
//! `content:` the text of the notes.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Fuzzy search over note names and paths.
    #[default]
    Files,
    /// Full-text search over the notes' text.
    Content,
}

const OPERATORS: [(&str, SearchMode); 3] = [("content:", SearchMode::Content), ("files:", SearchMode::Files), ("file:", SearchMode::Files)];

/// The mode a query asks for and the text to search: a leading `content:`, `file:` or
/// `files:` (any case) picks the mode; without one, file names are searched.
pub fn parse_search(query: &str) -> (SearchMode, &str) {
    let query = query.trim();
    for (operator, mode) in OPERATORS {
        if query.len() >= operator.len() && query.is_char_boundary(operator.len()) && query[..operator.len()].eq_ignore_ascii_case(operator) {
            return (mode, query[operator.len()..].trim());
        }
    }
    (SearchMode::Files, query)
}

/// `query` with its operator replaced by the one of `mode` (none for the default, files).
pub fn with_search_mode(query: &str, mode: SearchMode) -> String {
    let (_, text) = parse_search(query);
    match mode {
        SearchMode::Files => text.to_owned(),
        SearchMode::Content => format!("content: {text}"),
    }
}
