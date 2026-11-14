use std::sync::atomic::{AtomicBool, Ordering};

use knotes_core::{BlockHit, BoxError, NoteBlocks, NoteRows, NoteText};
use knotes_embed::{Embedder, Passage};

use crate::{VectorStore, block_passage};

/// Passages one request to the embedder takes: small enough that a search waiting behind it is
/// not kept long.
const BATCH: usize = 32;

/// Brings `store` up to the index: drops the vectors of documents gone, embeds the documents new
/// or changed since (the newest first), a batch at a time, until done or told to `stop`. How
/// many documents it embedded.
pub fn update(index: &(impl NoteRows + NoteText + NoteBlocks), store: &VectorStore, embedder: &dyn Embedder, stop: &AtomicBool) -> Result<usize, BoxError> {
    let mut rows = index.note_rows()?;
    let dates = store.dates()?;
    for path in dates.keys().filter(|p| !rows.iter().any(|r| &r.path == *p)) {
        store.remove(path)?;
    }
    rows.retain(|r| dates.get(&r.path) != Some(&r.mtime_ns));
    rows.sort_by_key(|r| std::cmp::Reverse(r.mtime_ns));
    let mut embedded = 0;
    // Documents gathered until their blocks fill a batch, then embedded together.
    let mut waiting: Vec<(String, i64, Vec<BlockHit>)> = Vec::new();
    let mut passages: Vec<Passage> = Vec::new();
    let mut rows = rows.into_iter().peekable();
    while let Some(row) = rows.next() {
        let text = index.note_text(&row.path)?.unwrap_or_default();
        let blocks = if text.is_empty() { Vec::new() } else { index.note_blocks(&row.path)? };
        passages.extend(blocks.iter().map(|b| block_passage(&row.path, &text, b)));
        waiting.push((row.path, row.mtime_ns, blocks));
        if passages.len() >= BATCH || rows.peek().is_none() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let mut vectors = embedder.embed(&passages)?.into_iter();
            for (path, mtime_ns, blocks) in waiting.drain(..) {
                let mine: Vec<_> = vectors.by_ref().take(blocks.len()).collect();
                store.put(&path, mtime_ns, &blocks, &mine)?;
                embedded += 1;
            }
            passages.clear();
        }
    }
    Ok(embedded)
}
