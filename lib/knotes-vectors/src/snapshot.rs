use knotes_core::BlockHit;
use knotes_embed::{Vector, similarity};

/// Blocks and their vectors, in memory, searched by brute force (256 bytes a block: 200 000
/// blocks are about 51 MB and a few milliseconds).
#[derive(Default)]
pub struct Snapshot {
    blocks: Vec<BlockHit>,
    vectors: Vec<Vector>,
}

impl Snapshot {
    pub fn push(&mut self, block: BlockHit, vector: Vector) {
        self.blocks.push(block);
        self.vectors.push(vector);
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The `limit` blocks nearest `query`, nearest first, each scored minus its similarity.
    pub fn nearest(&self, query: &Vector, limit: usize) -> Vec<BlockHit> {
        let mut scored: Vec<(i32, usize)> = self.vectors.iter().enumerate().map(|(i, v)| (similarity(query, v), i)).collect();
        let limit = limit.min(scored.len());
        if limit == 0 {
            return Vec::new();
        }
        scored.select_nth_unstable_by(limit - 1, |a, b| b.0.cmp(&a.0));
        scored.truncate(limit);
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(s, i)| BlockHit { score: -f64::from(s), ..self.blocks[i].clone() }).collect()
    }
}
