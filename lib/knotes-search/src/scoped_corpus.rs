use std::collections::HashSet;

use knotes_core::TextCorpus;

/// The notes of `inner` whose paths are in a scope, in `inner`'s order. Texts are not copied.
pub struct ScopedCorpus<'a, C: TextCorpus> {
    inner: &'a C,
    picked: Vec<usize>,
}

impl<'a, C: TextCorpus> ScopedCorpus<'a, C> {
    /// Paths that `inner` does not have are ignored.
    pub fn new(inner: &'a C, paths: &[String]) -> Self {
        let wanted: HashSet<&str> = paths.iter().map(String::as_str).collect();
        let picked = (0..inner.len()).filter(|&i| wanted.contains(inner.note(i).0)).collect();
        Self { inner, picked }
    }
}

impl<C: TextCorpus> TextCorpus for ScopedCorpus<'_, C> {
    fn len(&self) -> usize {
        self.picked.len()
    }

    fn note(&self, i: usize) -> (&str, &[u8]) {
        self.inner.note(self.picked[i])
    }
}
