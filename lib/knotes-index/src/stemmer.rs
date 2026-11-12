/// Reduces a query word to a stem that is then searched as a prefix.
pub(crate) trait Stemmer {
    /// Returns a prefix of `word` (lowercase). Returning `word` itself means "no ending found".
    fn stem<'a>(&self, word: &'a str) -> &'a str;
}
