//! List search in a background thread with its own read-only index connection (ADR 0015):
//! typing in the search field never waits for the title matcher, full-text search or search by
//! meaning (ADR 0028), which reads the vectors Knotes keeps beside the index.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use knotes_app::{ContentMatch, SearchMode, found_snippet};
use knotes_core::{ContentQuery, ContentSearch, MeaningSearch, NoteText, TitleSearch, Within};
use knotes_embed::Embedder;
use knotes_index::SqliteIndex;
use knotes_search::TitleIndex;
use knotes_vectors::{MeaningIndex, VectorStore};

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
}

pub struct SearchResponse {
    pub generation: u64,
    /// Paths whose title matched, best first.
    pub titles: Vec<String>,
    /// Notes whose text matched, best first, with the text around the match.
    pub content: Vec<ContentMatch>,
}

pub struct SearchWorker {
    requests: Sender<SearchRequest>,
}

impl SearchWorker {
    /// Starts the thread; `deliver` runs on it with each answer. The thread ends when the
    /// worker is dropped.
    pub fn start(index_dir: PathBuf, deliver: impl Fn(SearchResponse) + Send + 'static) -> Self {
        let (requests, incoming) = channel::<SearchRequest>();
        std::thread::Builder::new()
            .name("knotes-search".into())
            .spawn(move || {
                let mut reader: Option<SqliteIndex> = None;
                let mut meaning: Option<MeaningIndex> = None;
                while let Ok(mut request) = incoming.recv() {
                    // Queries typed meanwhile replace this one: only the latest matters.
                    while let Ok(newer) = incoming.try_recv() {
                        request = newer;
                    }
                    if reader.is_none() {
                        reader = SqliteIndex::open_reader(&index_dir).ok();
                    }
                    if meaning.is_none() && request.mode == SearchMode::Meaning {
                        meaning = crate::meaning::embedder()
                            .and_then(|embedder| VectorStore::open_reader(&index_dir).ok().map(|store| MeaningIndex::over_store(embedder as Arc<dyn Embedder>, store)));
                    }
                    deliver(search(&request, reader.as_ref(), meaning.as_ref().map(|m| m as &dyn MeaningSearch)));
                }
            })
            .expect("search thread starts");
        Self { requests }
    }

    pub fn request(&self, request: SearchRequest) {
        let _ = self.requests.send(request);
    }
}

fn search(request: &SearchRequest, reader: Option<&SqliteIndex>, meaning: Option<&dyn MeaningSearch>) -> SearchResponse {
    let query = request.query.trim();
    let mut titles = Vec::new();
    let mut content = Vec::new();
    let within = match request.mode {
        SearchMode::Files | SearchMode::Meaning => None,
        SearchMode::Content => Some(Within::Note),
        SearchMode::Block => Some(Within::Block),
    };
    if request.mode == SearchMode::Meaning {
        // The notes nearest in meaning, each once, its nearest block shown.
        if let (Some(reader), Some(meaning), false) = (reader, meaning, query.is_empty())
            && let Ok(hits) = meaning.nearest(query, MAX_MEANING, request.scope.as_deref())
        {
            let mut seen = HashSet::new();
            for hit in hits.into_iter().filter(|h| seen.insert(h.path.clone())) {
                let text = reader.note_text(&hit.path).ok().flatten().unwrap_or_default();
                let block = text.get(hit.span.start as usize..hit.span.end as usize).unwrap_or(&text);
                content.push(ContentMatch { snippet: found_snippet(block, query, SNIPPET_CHARS), path: hit.path });
            }
        }
    } else if within.is_none() {
        titles = request.titles.as_ref().map(|t| t.search_titles(query, MAX_TITLES).into_iter().map(|h| h.path).collect()).unwrap_or_default();
    } else if let (Some(reader), Some(within)) = (reader, within)
        && let Ok(hits) = reader.search(&ContentQuery { text: query, within, prefixes: true, scope: request.scope.as_deref(), limit: MAX_BLOCKS })
    {
        let mut seen = HashSet::new();
        for hit in hits {
            if !seen.insert(hit.path.clone()) {
                continue;
            }
            let text = reader.note_text(&hit.path).ok().flatten().unwrap_or_default();
            let block = text.get(hit.span.start as usize..hit.span.end as usize).unwrap_or(&text);
            content.push(ContentMatch { snippet: found_snippet(block, query, SNIPPET_CHARS), path: hit.path });
        }
    }
    SearchResponse { generation: request.generation, titles, content }
}

#[cfg(test)]
mod tests {
    use knotes_app::SearchMode;
    use knotes_core::{BlockHit, BoxError, FileKind, IndexChange, IndexWriter, MeaningSearch, NoteParser, Span, VaultFile};
    use knotes_index::SqliteIndex;

    use super::{SearchRequest, search};

    /// Search by meaning as a list given beforehand, in the scope asked.
    struct Near(Vec<(&'static str, u32, u32)>);

    impl MeaningSearch for Near {
        fn nearest(&self, _query: &str, limit: usize, scope: Option<&[String]>) -> Result<Vec<BlockHit>, BoxError> {
            Ok(self
                .0
                .iter()
                .filter(|(path, ..)| scope.is_none_or(|s| s.iter().any(|p| p == path)))
                .take(limit)
                .map(|(path, start, end)| BlockHit { path: path.to_string(), ord: 0, span: Span { start: *start, end: *end }, heading: None, score: 0.0 })
                .collect())
        }
    }

    #[test]
    fn search_by_meaning_lists_the_nearest_notes_once_each_with_their_block() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        let notes = [("Cat.md", "Барсик їсть Royal Canin."), ("Shop.md", "Корм коту, сіль."), ("Work/Plan.md", "Квартальний план.")];
        let parsed: Vec<_> = notes.iter().map(|(_, t)| knotes_md::MarkdownParser::new().parse(t)).collect();
        let files: Vec<_> = notes.iter().map(|(p, _)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
        let changes: Vec<_> = files.iter().zip(&notes).zip(&parsed).map(|((file, (_, text)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
        index.apply(&changes).unwrap();
        let whole = |i: usize| notes[i].1.len() as u32;
        let near = Near(vec![("Cat.md", 0, whole(0)), ("Cat.md", 0, 6), ("Work/Plan.md", 0, whole(2)), ("Shop.md", 0, whole(1))]);
        let request = |scope: Option<Vec<String>>| SearchRequest { generation: 1, mode: SearchMode::Meaning, query: "чим годувати кота".into(), scope, titles: None };
        let found = search(&request(None), Some(&index), Some(&near));
        let paths: Vec<&str> = found.content.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["Cat.md", "Work/Plan.md", "Shop.md"], "nearest first, each note once");
        assert!(found.content[0].snippet.contains("Royal Canin"), "{:?}", found.content[0].snippet);
        let in_list = search(&request(Some(vec!["Cat.md".into(), "Shop.md".into()])), Some(&index), Some(&near));
        assert_eq!(in_list.content.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["Cat.md", "Shop.md"], "the selected list's notes only");
        assert!(search(&request(None), Some(&index), None).content.is_empty(), "no model: nothing found, nothing broken");
    }
}
