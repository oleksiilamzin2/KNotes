//! Search in the note list (ADR 0015): title matches first, then notes whose text matched,
//! within the list the sidebar selected.

use std::collections::HashSet;

use knotes_core::NoteRow;

use crate::note_list::keeps;
use crate::{ListFilter, ListItem, ListSettings};

/// A note whose text matched, with the text around the match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub path: String,
    pub snippet: String,
}

/// Results in the list of `filter`: notes in `titles` (best first), each with its text match
/// when there is one, then the other notes of `content` (best first). `rows` is sorted by path.
pub fn search_list(rows: &[NoteRow], filter: &ListFilter, settings: &ListSettings, titles: &[String], content: &[ContentMatch]) -> Vec<ListItem> {
    let row_of = |path: &str| rows.binary_search_by(|r| r.path.as_str().cmp(path)).ok().filter(|&i| keeps(filter, settings, &rows[i]));
    let snippet_of = |path: &str| content.iter().find(|m| m.path == path).map(|m| m.snippet.clone());
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for path in titles {
        if let Some(i) = row_of(path)
            && seen.insert(i)
        {
            items.push(match snippet_of(path) {
                Some(snippet) => ListItem::Found(i as u32, snippet),
                None => ListItem::Note(i as u32),
            });
        }
    }
    for m in content {
        if let Some(i) = row_of(&m.path)
            && seen.insert(i)
        {
            items.push(ListItem::Found(i as u32, m.snippet.clone()));
        }
    }
    items
}

/// About `max_chars` of `text` on one line, from a little before the first word that starts
/// like a query word (word forms: the first two thirds of longer words), case-insensitive;
/// Markdown emphasis, code and link brackets are dropped. Without a match, the start of the text.
pub fn found_snippet(text: &str, query: &str, max_chars: usize) -> String {
    let plain = flatten(text);
    let chars: Vec<char> = plain.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let prefixes: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|w| {
            let w: Vec<char> = w.to_lowercase().chars().collect();
            let keep = if w.len() > 3 { (w.len() * 2 / 3).max(3) } else { w.len() };
            w[..keep].to_vec()
        })
        .filter(|p| !p.is_empty())
        .collect();
    let at_word_start = |i: usize| i == 0 || !lower[i - 1].is_alphanumeric();
    let found = (0..lower.len()).find(|&i| at_word_start(i) && prefixes.iter().any(|p| lower[i..].starts_with(p)));
    // A couple of words of context before the match: list rows show about 45 characters.
    let mut start = found.map_or(0, |i| i.saturating_sub((max_chars / 8).min(16)));
    while start > 0 && !at_word_start(start) {
        start -= 1;
    }
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    let end = (start + max_chars).min(chars.len());
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// The text on one line, without emphasis, code, link brackets and heading or quote markers.
fn flatten(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let line = line.trim_start().trim_start_matches(['#', '>']).trim();
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(line.chars().filter(|c| !matches!(c, '*' | '`' | '[' | ']')));
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
