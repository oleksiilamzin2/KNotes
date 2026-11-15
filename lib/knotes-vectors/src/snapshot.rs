use std::collections::HashSet;

use knotes_core::BlockHit;
use knotes_embed::{Vector, similarity};

/// Blocks and their vectors, in memory, searched by brute force (256 bytes a block: 200 000
/// blocks are about 51 MB and a few milliseconds).
#[derive(Default, Clone)]
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
    /// documents of `scope` when given, nearest first. A block embedded in pieces comes once, as
    /// its nearest piece. Each is scored by how far it stands out from all the blocks searched:
    /// minus its similarity's distance from their mean, in their standard deviations. Raw
    /// similarity says little (e5 scores unrelated notes 0.7–0.8, EmbeddingGemma 2 0.2–0.4); a
    /// block that answers the query stands out from the crowd, and with no answer none does.
    pub fn nearest(&self, query: &Vector, limit: usize, per_document: usize, scope: Option<&HashSet<&str>>) -> Vec<BlockHit> {
        let mut scored: Vec<(i32, usize)> = self
            .vectors
            .iter()
            .enumerate()
            .filter(|(i, _)| scope.is_none_or(|paths| paths.contains(self.blocks[*i].path.as_str())))
            .map(|(i, v)| (similarity(query, v), i))
            .collect();
        let n = scored.len().max(1) as f64;
        let mean = scored.iter().map(|(s, _)| f64::from(*s)).sum::<f64>() / n;
        let spread = (scored.iter().map(|(s, _)| (f64::from(*s) - mean).powi(2)).sum::<f64>() / n).sqrt();
        let standing = |s: i32| if spread < 1e-9 { 0.0 } else { (f64::from(s) - mean) / spread };
        // Enough candidates that the cap per document still leaves `limit`, as a rule.
        let candidates = (limit * 8).min(scored.len());
        if candidates == 0 || limit == 0 {
            return Vec::new();
        }
        scored.select_nth_unstable_by(candidates - 1, |a, b| b.0.cmp(&a.0));
        scored.truncate(candidates);
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut taken: Vec<(&str, usize)> = Vec::new();
        let mut blocks: HashSet<(&str, u32)> = HashSet::new();
        let mut found = Vec::with_capacity(limit);
        for (s, i) in scored {
            let path = self.blocks[i].path.as_str();
            if !blocks.insert((path, self.blocks[i].ord)) {
                continue;
            }
            let count = match taken.iter_mut().find(|(p, _)| *p == path) {
                Some((_, n)) => n,
                None => {
                    taken.push((path, 0));
                    &mut taken.last_mut().expect("just pushed").1
                }
            };
            if *count < per_document {
                *count += 1;
                found.push(BlockHit { score: -standing(s), ..self.blocks[i].clone() });
                if found.len() == limit {
                    break;
                }
            }
        }
        found
    }
}
