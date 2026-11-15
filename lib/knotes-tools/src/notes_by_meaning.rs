//! The note list's search by meaning (ADR 0015, ADR 0028): the notes holding the query's words
//! first, then those near in meaning, the far ones left out. In the tools' crate so the retrieval
//! benchmark measures it as it measures the tools' search.

use knotes_core::{BlockHit, ContentQuery, ContentSearch, MeaningSearch, Within};

use crate::without_question_words;

/// How near in meaning a block must be (cosine) to be listed when the query's words are not in
/// its note...
pub const NEAR_ENOUGH: f64 = 0.25;
/// ...and no farther than this below the nearest. Tuned on the benchmark's 65 queries by meaning
/// (2026-11-15): a floor of 0.30 left five queries with nothing (other languages score lower),
/// 0.25 found 54 of 65 as the tools' search does; behind the nearest by 0.25 a query listed 16
/// documents, by 0.10 5.5 with nothing lost, by 0.07 one answer was lost.
pub const BEHIND_NEAREST: f64 = 0.10;

/// Search by meaning in the list: the notes holding the query's words (question and function
/// words left out, word forms found) first, as search by words would give them, then merged by
/// reciprocal rank fusion with the notes near in meaning, those not near enough left out; each
/// note once, as the block that found it. A note saying "тиск" and "ліки" comes before one
/// merely about medicines, which the model alone put first. `limit` bounds each list.
pub fn notes_by_meaning(index: &impl ContentSearch, meaning: &dyn MeaningSearch, query: &str, scope: Option<&[String]>, limit: usize) -> Vec<BlockHit> {
    const K: f64 = 60.0;
    let words = without_question_words(query);
    let lexical = index.search(&ContentQuery { text: &words, within: Within::Note, prefixes: true, scope, limit }).unwrap_or_default();
    let near = meaning.nearest(query, limit, scope).unwrap_or_default();
    let full = 127.0 * 127.0;
    let nearest = near.first().map_or(0.0, |h| -h.score / full);
    let near_enough = NEAR_ENOUGH.max(nearest - BEHIND_NEAREST) * full;
    // Each note's best block and its score; the first note by words stays first.
    let mut found: Vec<(BlockHit, f64)> = Vec::new();
    let mut add = |hit: BlockHit, rank: usize| {
        let score = 1.0 / (K + rank as f64 + 1.0);
        match found.iter_mut().find(|(h, _)| h.path == hit.path) {
            Some((_, total)) => *total += score,
            None => found.push((hit, score)),
        }
    };
    let first = lexical.first().map(|h| h.path.clone());
    for (rank, hit) in lexical.into_iter().enumerate() {
        add(hit, rank);
    }
    let mut seen = std::collections::HashSet::new();
    for (rank, hit) in near.into_iter().filter(|h| -h.score >= near_enough && seen.insert(h.path.clone())).enumerate() {
        add(hit, rank);
    }
    if let Some((_, score)) = found.iter_mut().find(|(h, _)| first.as_ref() == Some(&h.path)) {
        *score = f64::INFINITY;
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.into_iter().map(|(hit, _)| hit).collect()
}
