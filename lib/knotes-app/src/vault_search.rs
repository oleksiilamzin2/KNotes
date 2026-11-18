//! What a search finds (ADR 0015): names by the title matcher, text by the index, notes near in
//! meaning (ADR 0028), narrowed as the query says (`Refine`). No window and no thread here: the
//! app's search thread calls it, and so do the tests.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use knotes_core::{ContentQuery, ContentSearch, MeaningSearch, NoteText, TagIndex, TitleSearch, Within};
use knotes_search::TitleIndex;

use crate::{ContentMatch, Place, SearchMode, found_snippet};

/// Most title matches and text blocks one search looks at (the list shows each note once).
const MAX_TITLES: usize = 2000;
const MAX_BLOCKS: usize = 500;
/// Blocks search by meaning gives (two at most of a note): past the first few dozen, every note
/// is somewhat near.
const MAX_MEANING: usize = 40;
/// Length of the found text shown in a result row.
const SNIPPET_CHARS: usize = 120;

pub struct SearchRequest {
    /// Increases with every request; only the answer to the latest one is shown.
    pub generation: u64,
    /// File names (fuzzy) or the notes' text (full-text).
    pub mode: SearchMode,
    /// The text to search, without the operator.
    pub query: String,
    /// Notes of the selected list; `None` for all notes.
    pub scope: Option<Vec<String>>,
    pub titles: Option<Arc<TitleIndex>>,
    /// Which finds by meaning to show (the chosen model's scale, ADR 0029).
    pub closeness: knotes_tools::Closeness,
    /// In a space, the list's notes in it: search by meaning finds there apart, so the rest
    /// of the vault can neither take its places nor raise its bar (ADR 0030).
    pub space: Option<Vec<String>>,
    /// Whether the query's case is ignored (the row's «Aa» on, as it starts). Kept, a name or a
    /// text found must hold each of the query's words as typed; search by meaning has no case.
    pub ignore_case: bool,
    /// With a search of the text, the names too (`titles` gives them): the vault's search
    /// window lists both at once. The list's search asks for one or the other.
    pub names_too: bool,
    /// What the query itself narrows the search by (a phrase, a word left out, a tag, a folder,
    /// a space): the vault's search window reads these; the list's search has none.
    pub refine: crate::Refine,
    /// The vault's spaces, for a search narrowed to one.
    pub spaces: Option<Arc<knotes_spaces::Spaces>>,
}

pub struct SearchResponse {
    pub generation: u64,
    /// Paths whose title matched, best first.
    pub titles: Vec<String>,
    /// Notes whose text matched, best first, with the text around the match.
    pub content: Vec<ContentMatch>,
}

/// The answer to `request`, read from `reader` (the index); `meaning` is the model's search,
/// none while there is no model.
pub fn search<R: ContentSearch + NoteText + TagIndex>(request: &SearchRequest, reader: Option<&R>, meaning: Option<&dyn MeaningSearch>) -> SearchResponse {
    // `OR` between words (the vault's search window): a search for each way to take one of
    // them, what they find listed together, each document once. By meaning, all the words at
    // once: nearness has no «or».
    let queries = request.refine.queries();
    if queries.len() < 2 || request.mode == SearchMode::Meaning {
        return search_words(request, request.query.trim(), reader, meaning);
    }
    let mut all = SearchResponse { generation: request.generation, titles: Vec::new(), content: Vec::new() };
    for query in &queries {
        let found = search_words(request, query, reader, meaning);
        all.titles.extend(found.titles.into_iter().filter(|path| !all.titles.contains(path)).collect::<Vec<_>>());
        all.content.extend(found.content.into_iter().filter(|found| !all.content.iter().any(|c| c.path == found.path)).collect::<Vec<_>>());
    }
    all
}

/// What `query`'s words find, all of them asked for, narrowed as the request says.
fn search_words<R: ContentSearch + NoteText + TagIndex>(request: &SearchRequest, query: &str, reader: Option<&R>, meaning: Option<&dyn MeaningSearch>) -> SearchResponse {
    let mut titles = Vec::new();
    let mut content = Vec::new();
    let within = match request.mode {
        SearchMode::Files | SearchMode::Meaning => None,
        SearchMode::Content => Some(Within::Note),
        SearchMode::Block => Some(Within::Block),
    };
    if request.mode == SearchMode::Meaning {
        if let (Some(reader), Some(meaning), false) = (reader, meaning, query.is_empty()) {
            content = match &request.space {
                Some(space) => {
                    let mut found = by_meaning(reader, meaning, query, Some(space), request.closeness);
                    let seen: HashSet<String> = found.iter().map(|m| m.path.clone()).collect();
                    found.extend(by_meaning(reader, meaning, query, request.scope.as_deref(), request.closeness).into_iter().filter(|m| !seen.contains(&m.path)));
                    found
                }
                None => by_meaning(reader, meaning, query, request.scope.as_deref(), request.closeness),
            };
        }
    }
    if request.mode == SearchMode::Files || request.names_too && within.is_some() {
        titles = request.titles.as_ref().map(|t| t.search_titles(query, MAX_TITLES).into_iter().map(|h| h.path).collect()).unwrap_or_default();
        if !request.ignore_case {
            titles.retain(|path: &String| holds_as_typed(crate::title_of(path), query));
        }
    }
    // The notes of each tag asked for. Where a tag must be had (it stands alone, or with other
    // tags only, in its list), only such notes are searched, so the most blocks looked at are
    // theirs.
    let refine = &request.refine;
    let with_tag: HashMap<&str, HashSet<String>> = refine.tags().into_iter().map(|tag| (tag, reader.and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect())).collect();
    let mut tagged: Option<HashSet<&String>> = None;
    for any in refine.places.iter().filter(|any| any.iter().all(|place| matches!(place, Place::Tag(_)))) {
        let notes: HashSet<&String> = any.iter().filter_map(|place| if let Place::Tag(tag) = place { with_tag.get(tag.as_str()) } else { None }).flatten().collect();
        tagged = Some(tagged.map_or(notes.clone(), |t| t.intersection(&notes).copied().collect()));
    }
    let tagged: Option<Vec<String>> = tagged.map(|notes| notes.into_iter().filter(|path| request.scope.as_ref().is_none_or(|scope| scope.contains(path))).cloned().collect());
    let scope = tagged.as_deref().or(request.scope.as_deref());
    if request.mode == SearchMode::Meaning || query.is_empty() {
    } else if let (Some(reader), Some(within)) = (reader, within)
        && let Ok(hits) = reader.search(&ContentQuery { text: query, within, prefixes: true, scope, limit: MAX_BLOCKS })
    {
        let mut seen = HashSet::new();
        for hit in hits {
            if !seen.insert(hit.path.clone()) {
                continue;
            }
            let text = reader.note_text(&hit.path).ok().flatten().unwrap_or_default();
            let block = text.get(hit.span.start as usize..hit.span.end as usize).unwrap_or(&text);
            // The index finds any case and any form of a word: with the case kept, only a text
            // (the block, when blocks are searched) holding the words as typed.
            if !request.ignore_case && !holds_as_typed(if request.mode == SearchMode::Block { block } else { &text }, query) {
                continue;
            }
            content.push(ContentMatch { snippet: found_snippet(block, query, SNIPPET_CHARS), path: hit.path });
        }
    }
    if refine.narrows() {
        let reads = !(refine.phrases.is_empty() && refine.without.is_empty());
        let keeps = |path: &String| {
            let space = request.spaces.as_ref().and_then(|spaces| spaces.of(path)).map(|space| space.name.as_str());
            refine.in_places(path, space, |tag| with_tag.get(tag).is_some_and(|notes| notes.contains(path)))
                && (!reads || refine.holds(crate::title_of(path), &reader.and_then(|r| r.note_text(path).ok().flatten()).unwrap_or_default(), request.ignore_case))
        };
        if query.is_empty() && !refine.places.is_empty() && request.mode != SearchMode::Meaning {
            // Where to look and no words: every document there, by its name.
            titles = request.titles.as_ref().map(|t| t.paths().iter().filter(|path| keeps(path)).take(MAX_TITLES).cloned().collect()).unwrap_or_default();
            titles.sort_by(|a, b| crate::natural_cmp(a, b));
        } else {
            titles.retain(&keeps);
            content.retain(|found| keeps(&found.path));
        }
    }
    SearchResponse { generation: request.generation, titles, content }
}

/// Whether `text` holds each word of `query` in the case it was typed in.
fn holds_as_typed(text: &str, query: &str) -> bool {
    query.split_whitespace().all(|word| text.contains(word))
}

/// Search by meaning in the list (ADR 0015): `knotes_tools::notes_by_meaning`, each note shown
/// with the block that found it.
fn by_meaning<R: ContentSearch + NoteText>(reader: &R, meaning: &dyn MeaningSearch, query: &str, scope: Option<&[String]>, closeness: knotes_tools::Closeness) -> Vec<ContentMatch> {
    knotes_tools::notes_by_meaning(reader, meaning, query, scope, MAX_MEANING, closeness)
        .into_iter()
        .map(|hit| {
            let text = reader.note_text(&hit.path).ok().flatten().unwrap_or_default();
            let block = text.get(hit.span.start as usize..hit.span.end as usize).unwrap_or(&text);
            ContentMatch { snippet: found_snippet(block, query, SNIPPET_CHARS), path: hit.path }
        })
        .collect()
}
