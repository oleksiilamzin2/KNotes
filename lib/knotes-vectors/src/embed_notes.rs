use knotes_core::{BoxError, NoteBlocks, NoteRows, NoteText};
use knotes_embed::Embedder;

use crate::{Snapshot, block_passage};

/// Passages one request to the embedder takes.
const BATCH: usize = 64;

/// Every indexed note's and PDF's blocks embedded, at once (for a small vault or a benchmark).
pub fn embed_notes(index: &(impl NoteRows + NoteText + NoteBlocks), embedder: &dyn Embedder) -> Result<Snapshot, BoxError> {
    let mut blocks = Vec::new();
    let mut passages = Vec::new();
    for row in index.note_rows()? {
        let Some(text) = index.note_text(&row.path)? else { continue };
        for block in index.note_blocks(&row.path)? {
            passages.push(block_passage(&row.path, &text, &block));
            blocks.push(block);
        }
    }
    let mut snapshot = Snapshot::default();
    for (chunk, passages) in blocks.chunks(BATCH).zip(passages.chunks(BATCH)) {
        for (block, vector) in chunk.iter().zip(embedder.embed(passages)?) {
            snapshot.push(block.clone(), vector);
        }
    }
    Ok(snapshot)
}
