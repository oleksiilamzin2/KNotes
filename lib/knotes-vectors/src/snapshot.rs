use std::collections::HashSet;

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

    /// The `limit` blocks nearest `query`, no more than `per_document` of one document, among the
    /// documents of `scope` when given, nearest first, each scored minus its similarity.
    pub fn nearest(&self, query: &Vector, limit: usize, per_document: usize, scope: Option<&HashSet<&str>>) -> Vec<BlockHit> {
        let mut scored: Vec<(i32, usize)> = self
            .vectors
            .iter()
            .enumerate()
            .filter(|(i, _)| scope.is_none_or(|paths| paths.contains(self.blocks[*i].path.as_str())))
            .map(|(i, v)| (similarity(query, v), i))
            .collect();
        // Enough candidates that the cap per document still leaves `limit`, as a rule.
        let candidates = (limit * 8).min(scored.len());
        if candidates == 0 || limit == 0 {
            return Vec::new();
        }
        scored.select_nth_unstable_by(candidates - 1, |a, b| b.0.cmp(&a.0));
        scored.truncate(candidates);
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut taken: Vec<(&str, usize)> = Vec::new();
        let mut found = Vec::with_capacity(limit);
        for (s, i) in scored {
            let path = self.blocks[i].path.as_str();
            let count = match taken.iter_mut().find(|(p, _)| *p == path) {
                Some((_, n)) => n,
                None => {
                    taken.push((path, 0));
                    &mut taken.last_mut().expect("just pushed").1
                }
            };
            if *count < per_document {
                *count += 1;
                found.push(BlockHit { score: -f64::from(s), ..self.blocks[i].clone() });
                if found.len() == limit {
                    break;
                }
            }
        }
        found
    }
}
