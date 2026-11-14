/// A text to embed: a query, or a block of a note with its note's title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passage {
    Query(String),
    Document { title: String, text: String },
}

impl Passage {
    /// The text as the model was trained to take it (EmbeddingGemma's model card).
    pub fn prompt(&self) -> String {
        match self {
            Self::Query(text) => format!("task: search result | query: {text}"),
            Self::Document { title, text } => format!("title: {} | text: {text}", if title.is_empty() { "none" } else { title }),
        }
    }
}
