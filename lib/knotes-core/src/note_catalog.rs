use crate::BoxError;

/// A vault file with the aliases from its frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub path: String,
    pub aliases: Vec<String>,
}

/// Every indexed file, for building in-memory lookups such as title search.
pub trait NoteCatalog {
    /// Sorted by path; aliases in frontmatter order.
    fn catalog(&self) -> Result<Vec<CatalogEntry>, BoxError>;
}
