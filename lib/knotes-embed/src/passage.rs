/// A text to embed: a query, or a block of a note with its note's title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passage {
    Query(String),
    Document { title: String, text: String },
}

/// How a model was trained to take its text (each model card's prompts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptStyle {
    /// EmbeddingGemma (1 and 2): `task: search result | query: …`, `title: … | text: …`.
    #[default]
    EmbeddingGemma,
    /// multilingual-e5-large-instruct: an English instruction before a query, a document as it is.
    E5Instruct,
}

impl Passage {
    /// The text as EmbeddingGemma takes it.
    pub fn prompt(&self) -> String {
        self.prompt_in(PromptStyle::EmbeddingGemma)
    }

    /// The text as a model of `style` takes it.
    pub fn prompt_in(&self, style: PromptStyle) -> String {
        match (style, self) {
            (PromptStyle::EmbeddingGemma, Self::Query(text)) => format!("task: search result | query: {text}"),
            (PromptStyle::EmbeddingGemma, Self::Document { title, text }) => format!("title: {} | text: {text}", if title.is_empty() { "none" } else { title }),
            (PromptStyle::E5Instruct, Self::Query(text)) => format!("Instruct: Given a question about the user's notes, retrieve the passages of the notes that answer it\nQuery: {text}"),
            (PromptStyle::E5Instruct, Self::Document { title, text }) if !title.is_empty() => format!("{title}\n{text}"),
            (PromptStyle::E5Instruct, Self::Document { text, .. }) => text.clone(),
        }
    }
}
