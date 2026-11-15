use crate::BlockHit;

/// Which finds by meaning search gives (the list's when the query's words are not in their
/// notes, the tools' beside those by words): those standing out from all the blocks by at least
/// `near_enough` standard deviations, and no more than `behind_nearest` of them behind the
/// nearest (`MeaningSearch` scores are minus how far a block stands out; `MeaningModel`'s, tuned
/// on the benchmark).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Closeness {
    pub near_enough: f64,
    pub behind_nearest: f64,
}

impl Closeness {
    /// The finds by meaning near enough, in the order they came (nearest first).
    pub fn keep(&self, near: Vec<BlockHit>) -> Vec<BlockHit> {
        let nearest = near.first().map_or(0.0, |h| -h.score);
        let near_enough = self.near_enough.max(nearest - self.behind_nearest);
        near.into_iter().filter(|h| -h.score >= near_enough).collect()
    }
}
