use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use knotes_core::{BlockHit, BoxError, NoteBlocks, NoteRows, NoteText};
use knotes_embed::{DIMS, Embedder, Passage, Vector};

use crate::{PIECE_CHARS, VectorStore, block_pieces};

/// Passages one request to the embedder takes: small enough that a search waiting behind it is
/// not kept long.
const BATCH: usize = 32;

/// How long a document must stay unchanged before it is embedded in the background: one being
/// written (saved every two seconds) would otherwise go to the model again and again while the
/// user types.
pub const SETTLE: Duration = Duration::from_secs(15);

/// How long a document under active editing must stay unchanged: one changed again within
/// `EDITING` of the version last embedded.
pub const SETTLE_EDITING: Duration = Duration::from_secs(60);

/// A document changed again this soon after the version last embedded is being worked on.
pub const EDITING: Duration = Duration::from_secs(5 * 60);

/// Which of the documents new or changed since they were embedded a pass embeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Those unchanged for `SETTLE` (`SETTLE_EDITING` under active editing): the background's
    /// passes.
    Settled,
    /// Those still settling, at once: just before the chat asks, so search sees what the user has
    /// just written; the backlog of first indexing stays the background's.
    Waiting,
}

/// What a pass did, and when to come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Updated {
    /// Documents whose vectors were written.
    pub documents: usize,
    /// Blocks that went to the model (those whose text had not changed did not).
    pub passages: usize,
    /// When the first document left for being written settles; none if no document was.
    pub next: Option<Duration>,
}

/// How far a pass has gone: documents embedded of those it found to embed, and the blocks that
/// went to the model so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub passages: usize,
}

/// A document on its way to the store: its blocks, the hash of each one's text, and each one's
/// vector, kept or still to come from the model.
struct Pending {
    path: String,
    mtime_ns: i64,
    blocks: Vec<BlockHit>,
    hashes: Vec<u64>,
    vectors: Vec<Option<Vector>>,
}

/// Brings `store` up to the index at `now_ns` (nanoseconds since 1970): drops the vectors of
/// documents gone, and embeds `which` of the documents new or changed since, the newest first,
/// a batch at a time, until done or told to `stop`. Only blocks whose text changed go to the
/// model; the rest keep their vectors.
pub fn update(index: &(impl NoteRows + NoteText + NoteBlocks), store: &VectorStore, embedder: &dyn Embedder, now_ns: i64, which: Which, stop: &AtomicBool) -> Result<Updated, BoxError> {
    update_reporting(index, store, embedder, now_ns, which, stop, &mut |_| {})
}

/// `update`, saying how far it has gone at its start and after each batch (for the settings'
/// progress bar).
pub fn update_reporting(
    index: &(impl NoteRows + NoteText + NoteBlocks),
    store: &VectorStore,
    embedder: &dyn Embedder,
    now_ns: i64,
    which: Which,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<Updated, BoxError> {
    let mut rows = index.note_rows()?;
    let dates = store.dates()?;
    let present: HashSet<&str> = rows.iter().map(|r| r.path.as_str()).collect();
    for path in dates.keys().filter(|p| !present.contains(p.as_str())) {
        store.remove(path)?;
    }
    rows.retain(|r| dates.get(&r.path) != Some(&r.mtime_ns));
    // Documents still settling wait for a settled pass; the first of them to settle says when to
    // come back. One changed again soon after its version embedded is under active editing and
    // settles longer.
    let nanos = |d: Duration| d.as_nanos() as i64;
    let mut next: Option<i64> = None;
    rows.retain(|r| {
        let editing = dates.get(&r.path).is_some_and(|embedded| r.mtime_ns - embedded < nanos(EDITING));
        let wait = nanos(if editing { SETTLE_EDITING } else { SETTLE }) - (now_ns - r.mtime_ns);
        match which {
            Which::Settled if wait > 0 => {
                next = Some(next.map_or(wait, |n| n.min(wait)));
                false
            }
            Which::Settled => true,
            Which::Waiting => wait > 0,
        }
    });
    rows.sort_by_key(|r| Reverse(r.mtime_ns));
    let mut done = Updated { documents: 0, passages: 0, next: next.map(|ns| Duration::from_nanos(ns as u64)) };
    let total = rows.len();
    if total > 0 {
        progress(Progress { done: 0, total, passages: 0 });
    }
    let (mut pending, mut asked, mut passages): (Vec<Pending>, Vec<(usize, usize)>, Vec<Passage>) = (Vec::new(), Vec::new(), Vec::new());
    let mut rows = rows.into_iter().peekable();
    while let Some(row) = rows.next() {
        let text = index.note_text(&row.path)?.unwrap_or_default();
        let blocks = if text.is_empty() { Vec::new() } else { index.note_blocks(&row.path)? };
        let kept = store.vectors_of(&row.path)?;
        let mut document = Pending { path: row.path.clone(), mtime_ns: row.mtime_ns, blocks: Vec::new(), hashes: Vec::new(), vectors: Vec::new() };
        for (block, passage) in blocks.iter().flat_map(|block| block_pieces(&row.path, &text, block, PIECE_CHARS)) {
            let hash = hash_of(&passage);
            let vector = kept.get(&hash).copied();
            if vector.is_none() {
                asked.push((pending.len(), document.vectors.len()));
                passages.push(passage);
            }
            document.blocks.push(block);
            document.hashes.push(hash);
            document.vectors.push(vector);
        }
        pending.push(document);
        if passages.len() >= BATCH || rows.peek().is_none() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !passages.is_empty() {
                for ((document, block), vector) in asked.drain(..).zip(embedder.embed(&passages)?) {
                    pending[document].vectors[block] = Some(vector);
                }
                done.passages += passages.len();
                passages.clear();
            }
            for document in pending.drain(..) {
                let vectors: Vec<Vector> = document.vectors.into_iter().map(|v| v.unwrap_or([0; DIMS])).collect();
                store.put(&document.path, document.mtime_ns, &document.blocks, &document.hashes, &vectors)?;
                done.documents += 1;
            }
            progress(Progress { done: done.documents, total, passages: done.passages });
        }
    }
    Ok(done)
}

/// FNV-1a over the text a passage is embedded as (its title too: a renamed note is embedded
/// again, as its blocks' text has changed).
fn hash_of(passage: &Passage) -> u64 {
    passage.prompt().bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
