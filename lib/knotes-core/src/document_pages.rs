/// Pages of vault files that are not notes (PDFs, ADR 0022), read one at a time, for the chat's
/// and the MCP server's tools (ADR 0027).
pub trait DocumentPages {
    /// How many pages the file at vault `path` has; `None` when it is no document read here.
    fn page_count(&self, path: &str) -> Option<usize>;
    /// The text of page `page` (index) of the file at vault `path`.
    fn page_text(&self, path: &str, page: usize) -> Option<String>;
    /// The file's own outline: each entry's level (1 for the top), title and page (index).
    fn outline(&self, path: &str) -> Vec<(u8, String, usize)>;
}

impl DocumentPages for crate::NoDocuments {
    fn page_count(&self, _path: &str) -> Option<usize> {
        None
    }

    fn page_text(&self, _path: &str, _page: usize) -> Option<String> {
        None
    }

    fn outline(&self, _path: &str) -> Vec<(u8, String, usize)> {
        Vec::new()
    }
}
