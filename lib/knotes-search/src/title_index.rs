use std::cmp::Reverse;
use std::collections::HashSet;

use knotes_core::{TitleHit, TitleSearch};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
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

    fn score(&self, entry: &Entry, pattern: &Pattern, matcher: &mut Matcher) -> Option<u32> {
        if let Some(s) = pattern.score(entry.title.slice(..), matcher) {
            return Some(s + TITLE_BONUS);
        }
        entry.path.as_ref().and_then(|p| pattern.score(p.slice(..), matcher))
    }
}

impl TitleSearch for TitleIndex {
    fn search_titles(&self, query: &str, limit: usize) -> Vec<TitleHit> {
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        if pattern.atoms.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, usize)> = self
            .entries
            .par_chunks(CHUNK)
            .enumerate()
            .flat_map_iter(|(chunk, entries)| {
                let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
                let pattern = &pattern;
                entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| self.score(e, pattern, &mut matcher).map(|s| (s, chunk * CHUNK + i)))
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
