use std::cmp::Reverse;
use std::collections::HashSet;

use knotes_core::{TitleHit, TitleSearch};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use rayon::prelude::*;

/// Added to a match inside the title (or alias), so it always ranks above a match that
/// needs folder names.
const TITLE_BONUS: u32 = 1 << 16;
/// Entries per rayon task; each task builds one matcher.
const CHUNK: usize = 2048;

struct Entry {
    /// Index into `paths`.
    file: u32,
    /// Title (file name without `.md`) or an alias.
    title: Utf32String,
    /// Path without `.md`; `None` for alias entries.
    path: Option<Utf32String>,
    alias: Option<u32>,
}

/// Titles, aliases and paths of all files, kept in memory as UTF-32 for the matcher.
pub struct TitleIndex {
    paths: Vec<String>,
    aliases: Vec<String>,
    entries: Vec<Entry>,
}

impl TitleIndex {
    /// `entries`: vault-relative path and aliases of each file.
    pub fn new(entries: impl IntoIterator<Item = (String, Vec<String>)>) -> Self {
        let mut index = Self { paths: Vec::new(), aliases: Vec::new(), entries: Vec::new() };
        for (path, aliases) in entries {
            let file = index.paths.len() as u32;
            let shown = path.strip_suffix(".md").unwrap_or(&path);
            let title = shown.rsplit('/').next().unwrap_or(shown);
            index.entries.push(Entry { file, title: title.into(), path: Some(shown.into()), alias: None });
            for alias in aliases {
                index.entries.push(Entry { file, title: alias.as_str().into(), path: None, alias: Some(index.aliases.len() as u32) });
                index.aliases.push(alias);
            }
            index.paths.push(path);
        }
        index
    }

    /// Every file's path, as given.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    fn score(&self, entry: &Entry, pattern: &Pattern, matcher: &mut Matcher) -> Option<u32> {
        if let Some(s) = pattern.score(entry.title.slice(..), matcher) {
            return Some(s + TITLE_BONUS);
        }
        entry.path.as_ref().and_then(|p| pattern.score(p.slice(..), matcher))
    }
}

impl TitleIndex {
    /// The files whose name, alias or path holds every word of `query` whole, letters in a row
    /// (any case), best first: what a search lists. `search_titles` matches letters apart too
    /// (`мама` found «Математика, мода»), which suits jumping to a note by a few of its
    /// letters, not a list of what was searched for.
    pub fn search_words(&self, query: &str, limit: usize) -> Vec<TitleHit> {
        // The matcher finds the names holding the words' letters in their order; of those, the
        // ones holding each word in a row are kept here. (Its own match of a word in a row
        // missed a word at a name's end, in letters outside ASCII: «аналізи» in «Мама аналізи».)
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.matching(&Pattern::new(query, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy), limit, Some(&words))
    }

    /// Whether the entry's name (or alias), or its path, holds each of `words` in a row.
    fn holds(entry: &Entry, words: &[String]) -> bool {
        let name = entry.title.to_string().to_lowercase();
        let path = entry.path.as_ref().map(|path| path.to_string().to_lowercase()).unwrap_or_default();
        words.iter().all(|word| name.contains(word) || path.contains(word))
    }

    fn matching(&self, pattern: &Pattern, limit: usize, words: Option<&[String]>) -> Vec<TitleHit> {
        if pattern.atoms.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, usize)> = self
            .entries
            .par_chunks(CHUNK)
            .enumerate()
            .flat_map_iter(|(chunk, entries)| {
                let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
                entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| self.score(e, pattern, &mut matcher).filter(|_| words.is_none_or(|words| Self::holds(e, words))).map(|s| (s, chunk * CHUNK + i)))
                    .collect::<Vec<_>>()
            })
            .collect();

        // Best score first; ties: shorter path, then path order.
        let key = |&(score, i): &(u32, usize)| {
            let path = &self.paths[self.entries[i].file as usize];
            (Reverse(score), path.len(), path.as_str())
        };
        // Aliases can repeat a path, so keep a margin before taking the top `limit`.
        let keep = limit.saturating_mul(4).max(limit + 16);
        if scored.len() > keep {
            scored.select_nth_unstable_by(keep, |a, b| key(a).cmp(&key(b)));
            scored.truncate(keep);
        }
        scored.sort_unstable_by(|a, b| key(a).cmp(&key(b)));

        let mut seen = HashSet::new();
        scored
            .into_iter()
            .filter(|&(_, i)| seen.insert(self.entries[i].file))
            .take(limit)
            .map(|(score, i)| {
                let e = &self.entries[i];
                TitleHit {
                    path: self.paths[e.file as usize].clone(),
                    alias: e.alias.map(|a| self.aliases[a as usize].clone()),
                    score,
                }
            })
            .collect()
    }
}

impl TitleSearch for TitleIndex {
    fn search_titles(&self, query: &str, limit: usize) -> Vec<TitleHit> {
        self.matching(&Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart), limit, None)
    }
}
