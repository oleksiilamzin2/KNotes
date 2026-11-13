/// Text of vault files that are not notes (PDFs, ADR 0022), for the index and the search.
pub trait DocumentText {
    /// The text of the file at vault `path`; `None` when it has none to give.
    fn document_text(&self, path: &str) -> Option<String>;
}

/// No file's text: notes only are indexed.
pub struct NoDocuments;

impl DocumentText for NoDocuments {
    fn document_text(&self, _path: &str) -> Option<String> {
        None
    }
}
