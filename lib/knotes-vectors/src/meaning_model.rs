use knotes_core::Closeness;


/// The setting turning search by meaning off.
pub const OFF: &str = "off";

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

    /// The model a setting names: none for `off` (search by meaning turned off), the default
    /// when there is no setting or one this version does not know.
    pub fn from_setting(setting: Option<&str>) -> Option<Self> {
        match setting {
            Some(OFF) => None,
            Some(key) => Some(Self::from_key(key).unwrap_or(Self::DEFAULT)),
            None => Some(Self::DEFAULT),
        }
    }

    /// The setting for `model` (none: off).
    pub fn setting(model: Option<Self>) -> &'static str {
        model.map_or(OFF, Self::key)
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
            Self::E5Large => "multilingual-e5-large-instruct-q8_0, first 512, title and heading › block in pieces of 400, dates in words, v2",
            Self::EmbeddingGemma2 => "embeddinggemma-2-Q8_0, first 512, title and heading › block in pieces of 400, dates in words, v2",
        }
    }

    /// For the note list's search by meaning and the tools' search: how far a block must stand out
    /// from all the blocks (in standard deviations) to be given by meaning, and how far behind the
    /// nearest it may be. Tuned on the benchmark (2026-11-15) with 14 queries the
    /// vault holds no answer to: cosine thresholds listed 11.8 (e5) and 15.3 (EmbeddingGemma 2)
    /// notes for them, more than for queries it answers, as unrelated notes crowd together; these
    /// list 3.9 and 4.9 and find as much (a floor of 1.9 cost e5 an answer, of 1.8 EmbeddingGemma 2).
    pub fn closeness(self) -> Closeness {
        match self {
            Self::E5Large => Closeness { near_enough: 1.8, behind_nearest: 1.6 },
            Self::EmbeddingGemma2 => Closeness { near_enough: 1.7, behind_nearest: 1.5 },
        }
    }
}
