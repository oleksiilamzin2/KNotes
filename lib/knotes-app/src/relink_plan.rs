//! What a rename or move changes in the vault's text, worked out before anything is written
//! (ADR 0019): the app asks "Update N links in M notes?" with it.

use std::collections::BTreeSet;

use knotes_core::{BoxError, LinkGraph, LinkResolver, NoteParser, OutLink};

use crate::{Moves, relink};

/// The notes whose links change, with their new text, and how many links change.
#[derive(Debug, Clone, Default)]
pub struct Relinks {
    /// Each edited note's path after the moves and its new text.
    pub edits: Vec<(String, String)>,
    pub links: usize,
}

/// The edits `moves` call for: in every note linking to a moved note (found through `graph`,
/// the index), and in the moved notes themselves. Links are parsed again from the text `read`
/// gives (the index may lag behind it) and resolved with `before`; `after` resolves links
/// once the moves are made.
pub fn plan_relinks(
    moves: &Moves,
    graph: &dyn LinkGraph,
    read: &dyn Fn(&str) -> Option<String>,
    parser: &dyn NoteParser,
    before: &dyn LinkResolver,
    after: &dyn LinkResolver,
) -> Result<Relinks, BoxError> {
    let mut sources: BTreeSet<String> = moves.keys().cloned().collect();
    for from in moves.keys() {
        sources.extend(graph.backlinks(from)?.into_iter().map(|b| b.source));
    }
    let mut plan = Relinks::default();
    for source in sources {
        let Some(text) = read(&source) else { continue };
        let links: Vec<OutLink> =
            parser.parse(&text).links.into_iter().map(|link| OutLink { resolved: before.resolve(link.kind, &link.target, &source), link }).collect();
        let (new, changed) = relink(&text, &links, &source, moves, after);
        if changed > 0 {
            plan.links += changed;
            plan.edits.push((moves.get(&source).cloned().unwrap_or(source), new));
        }
    }
    Ok(plan)
}
