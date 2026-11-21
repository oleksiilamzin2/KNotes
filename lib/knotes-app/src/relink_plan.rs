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
/// the index), in the moved notes themselves, and in the notes `fresh`, just written (the note
/// open, those it embeds), whose links the index may not know yet. Links are parsed again from
/// the text `read` gives (the index may lag behind it) and resolved with `before`; `after`
/// resolves links once the moves are made.
pub fn plan_relinks(
    moves: &Moves,
    graph: &dyn LinkGraph,
    read: &dyn Fn(&str) -> Option<String>,
    parser: &dyn NoteParser,
    before: &dyn LinkResolver,
    after: &dyn LinkResolver,
    fresh: &[String],
) -> Result<Relinks, BoxError> {
    let mut sources: BTreeSet<String> = moves.keys().chain(fresh).cloned().collect();
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

/// The edits that take every link to the files `gone` out of the vault's text: in every note
/// linking to one (found through `graph`) and in the notes `fresh`, just written. A link alone
/// on its line goes with its line, one in a line with one of the spaces around it (as a
/// deleted card does). Links are parsed again from the text `read` gives and resolved with
/// `links`.
pub fn plan_unlinks(gone: &[String], graph: &dyn LinkGraph, read: &dyn Fn(&str) -> Option<String>, parser: &dyn NoteParser, links: &dyn LinkResolver, fresh: &[String]) -> Result<Relinks, BoxError> {
    let mut sources: BTreeSet<String> = fresh.iter().cloned().collect();
    for path in gone {
        sources.extend(graph.backlinks(path)?.into_iter().map(|b| b.source));
    }
    let mut plan = Relinks::default();
    for source in sources {
        let Some(mut text) = read(&source) else { continue };
        let mut spans: Vec<std::ops::Range<usize>> = parser
            .parse(&text)
            .links
            .into_iter()
            .filter(|link| links.resolve(link.kind, &link.target, &source).is_some_and(|to| gone.contains(&to)))
            .map(|link| link.span.start as usize..link.span.end as usize)
            .collect();
        if spans.is_empty() {
            continue;
        }
        // From the end: what is cut does not move the spans before it.
        spans.sort_by_key(|span| std::cmp::Reverse(span.start));
        plan.links += spans.len();
        for span in spans {
            text.replace_range(crate::block_to_delete(&text, span), "");
        }
        plan.edits.push((source, text));
    }
    Ok(plan)
}
