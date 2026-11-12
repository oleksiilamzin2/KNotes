use crate::Rng;

/// Source of words for synthetic text. A real-text corpus can replace the vocabulary later.
pub(crate) trait WordSource {
    /// A word drawn with natural (Zipf) frequency, function words included.
    fn word(&self, rng: &mut Rng) -> &str;
    /// A moderately common content word, for titles, headings and tags.
    fn content_word(&self, rng: &mut Rng) -> &str;
}
