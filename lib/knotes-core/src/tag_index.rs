use crate::BoxError;

/// Tags are case-insensitive and stored lowercased, inline and frontmatter tags together.
pub trait TagIndex {
    /// Distinct tags of a note, sorted.
    fn tags_of(&self, path: &str) -> Result<Vec<String>, BoxError>;
    /// Paths of notes that have `tag` or a nested tag below it (`area` matches `area/sub`), sorted.
    fn notes_with_tag(&self, tag: &str) -> Result<Vec<String>, BoxError>;
    /// Every tag and every parent of a nested tag, with the number of distinct notes that
    /// have it or a tag below it; sorted by tag.
    fn tag_counts(&self) -> Result<Vec<(String, usize)>, BoxError>;
    /// Paths of notes without any tag, sorted.
    fn untagged_notes(&self) -> Result<Vec<String>, BoxError>;
}
