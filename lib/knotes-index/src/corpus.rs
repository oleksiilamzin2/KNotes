use std::sync::Arc;

use knotes_core::{BoxError, CorpusSource, TextCorpus};
use memmap2::Mmap;

use crate::SqliteIndex;

/// Paths and pack ranges of all notes at one moment, over a shared mapping of the pack.
/// The pack is append-only, so the mapped ranges never change under the snapshot.
pub struct TextSnapshot {
    map: Option<Arc<Mmap>>,
    notes: Vec<(String, usize, usize)>,
}

impl TextCorpus for TextSnapshot {
    fn len(&self) -> usize {
        self.notes.len()
    }

    fn note(&self, i: usize) -> (&str, &[u8]) {
        let (path, start, end) = &self.notes[i];
        (path, self.map.as_deref().map_or(&[][..], |m| &m[*start..*end]))
    }
}

impl CorpusSource for SqliteIndex {
    type Corpus = TextSnapshot;

    /// Cached until the next write.
    fn corpus(&self) -> Result<Arc<TextSnapshot>, BoxError> {
        if let Some(c) = self.corpus.borrow().as_ref() {
            return Ok(Arc::clone(c));
        }
        let mut st = self.conn.prepare_cached(
            "SELECT path, pack_off, pack_off + pack_len FROM files WHERE pack_off IS NOT NULL ORDER BY path",
        )?;
        let notes: Vec<(String, usize, usize)> =
            st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, i64>(2)? as usize)))?
                .collect::<Result<_, _>>()?;
        let end = notes.iter().map(|n| n.2).max().unwrap_or(0);
        let map = self.pack.map_covering(end as u64)?;
        let snapshot = Arc::new(TextSnapshot { map, notes });
        *self.corpus.borrow_mut() = Some(Arc::clone(&snapshot));
        Ok(snapshot)
    }
}
