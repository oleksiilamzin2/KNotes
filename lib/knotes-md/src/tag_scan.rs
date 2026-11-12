//! Obsidian inline tags inside a run of plain text.

use std::ops::Range;

use knotes_core::{Span, Tag};

/// Scans `source[run]` (a contiguous run of plain text) and appends found tags.
/// A tag starts with `#` at the start of the run or after whitespace, continues with
/// letters, digits, `_`, `-`, `/`, and is not made of digits only.
pub(crate) fn scan_tags(source: &str, run: Range<usize>, out: &mut Vec<Tag>) {
    let text = &source[run.clone()];
    for (i, _) in text.match_indices('#') {
        let at_boundary = if i == 0 {
            // pulldown-cmark starts the text of `\#` at `#`; the escape is only visible in the source.
            !source[..run.start].ends_with('\\')
        } else {
            text[..i].chars().next_back().is_some_and(char::is_whitespace)
        };
        if !at_boundary {
            continue;
        }
        let rest = &text[i + 1..];
        let len = rest.char_indices().find(|&(_, c)| !is_tag_char(c)).map_or(rest.len(), |(j, _)| j);
        let name = rest[..len].trim_end_matches('/');
        if name.is_empty() || name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let start = run.start + i;
        out.push(Tag { name: name.to_owned(), span: Span::new(start, start + 1 + name.len()) });
    }
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}
