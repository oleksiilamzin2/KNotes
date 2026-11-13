//! Find in the open note (Cmd+F): every occurrence of the typed text, whatever its case.

use std::ops::Range;

/// Byte ranges of the occurrences of `query` in `text`, case-insensitive, not overlapping.
pub fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    let wanted: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Lowercase the text from `i` on as far as the query reaches.
        let (mut matched, mut j) = (0, i);
        while matched < wanted.len() && j < chars.len() {
            let lower: Vec<char> = chars[j].1.to_lowercase().collect();
            if wanted[matched..].starts_with(&lower) {
                matched += lower.len();
                j += 1;
            } else {
                break;
            }
        }
        if matched == wanted.len() {
            let end = chars.get(j).map_or(text.len(), |c| c.0);
            found.push(chars[i].0..end);
            i = j;
        } else {
            i += 1;
        }
    }
    found
}
