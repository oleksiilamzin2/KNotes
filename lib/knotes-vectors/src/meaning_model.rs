/// The embedding models search by meaning can run on, the user's choice in the settings
/// (ADR 0029). Both ship in the bundle; each keeps its vectors in a store of its own, so going
/// back to one already embedded with is instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningModel {
    /// multilingual-e5-large-instruct (MIT): the more accurate, twice the size.
    E5Large,
    /// EmbeddingGemma 2 (Apache 2.0): the lighter.
    EmbeddingGemma2,
}

impl MeaningModel {
    pub const ALL: [Self; 2] = [Self::E5Large, Self::EmbeddingGemma2];
    /// The one search by meaning runs on until the user chooses: it found 63 of the benchmark's
    /// 65 queries by meaning, EmbeddingGemma 2 about 60.
    pub const DEFAULT: Self = Self::E5Large;

    /// What the settings call it.
    pub fn key(self) -> &'static str {
        match self {
            Self::E5Large => "e5-large-instruct",
            Self::EmbeddingGemma2 => "embeddinggemma-2",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// Its file, in the bundle's Resources (the repository's `models/` while developing).
    pub fn file(self) -> &'static str {
        match self {
            Self::E5Large => "multilingual-e5-large-instruct-q8_0.gguf",
            Self::EmbeddingGemma2 => "embeddinggemma-2-Q8_0.gguf",
        }
    }

    /// The store of its vectors, beside the vault's index.
    pub fn store_file(self) -> String {
        format!("vectors-{}.sqlite", self.key())
    }

    /// How its vectors are made, kept with them: another way, and the store is embedded again.
    pub fn embedding(self) -> &'static str {
        match self {
            Self::E5Large => "multilingual-e5-large-instruct-q8_0, first 512, title and heading › block, dates in words, v1",
            Self::EmbeddingGemma2 => "embeddinggemma-2-Q8_0, first 512, title and heading › block, dates in words, v1",
        }
    }

    /// For the note list's search by meaning: how near (cosine) a block must be to be listed when
    /// the query's words are not in its note, and how far behind the nearest it may be. Each
    /// model scores on a scale of its own, tuned on the benchmark (2026-11-15): e5 scores every
    /// note 0.7–0.9, so behind by 0.10 listed 31 documents a query, by 0.03 7 with nothing lost,
    /// by 0.025 one answer was lost; EmbeddingGemma 2 listed 18.6 by 0.10, 6.3 by 0.05, and lost
    /// one by 0.04.
    pub fn closeness(self) -> (f64, f64) {
        match self {
            Self::E5Large => (0.70, 0.03),
            Self::EmbeddingGemma2 => (0.30, 0.05),
        }
    }
}
