//! List search in a background thread with its own read-only index connection (ADR 0015):
//! typing in the search field never waits for the title matcher or full-text search.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use knotes_app::{ContentMatch, SearchMode, found_snippet};
use knotes_core::{ContentQuery, ContentSearch, NoteText, TitleSearch, Within};
use knotes_index::SqliteIndex;
use knotes_search::TitleIndex;

/// Most title matches and text blocks one search looks at (the list shows each note once).
const MAX_TITLES: usize = 2000;
const MAX_BLOCKS: usize = 500;
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
                while let Ok(mut request) = incoming.recv() {
                    // Queries typed meanwhile replace this one: only the latest matters.
                    while let Ok(newer) = incoming.try_recv() {
                        request = newer;
                    }
                    if reader.is_none() {
                        reader = SqliteIndex::open_reader(&index_dir).ok();
                    }
                    deliver(search(&request, reader.as_ref()));
                }
            })
            .expect("search thread starts");
        Self { requests }
    }

    pub fn request(&self, request: SearchRequest) {
        let _ = self.requests.send(request);
    }
}

fn search(request: &SearchRequest, reader: Option<&SqliteIndex>) -> SearchResponse {
    let query = request.query.trim();
    let mut titles = Vec::new();
    let mut content = Vec::new();
    let within = match request.mode {
        SearchMode::Files => None,
        SearchMode::Content => Some(Within::Note),
        SearchMode::Block => Some(Within::Block),
    };
    if within.is_none() {
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
