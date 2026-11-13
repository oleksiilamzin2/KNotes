//! Bare web addresses in plain text (`https://example.com`): links to websites, as Obsidian
//! makes them.

use std::ops::Range;

use knotes_core::{Link, LinkKind, Span};

/// Scans `source[run]` (a contiguous run of plain text) and appends the `http://` and
/// `https://` addresses in it as external links. An address ends at white space or `<>"`;
/// punctuation after it (`.`, `,`, a `)` without its `(`) is not part of it.
pub(crate) fn scan_urls(source: &str, run: Range<usize>, out: &mut Vec<Link>) {
    let text = &source[run.clone()];
    let mut from = 0;
    while let Some(found) = ["https://", "http://"].iter().filter_map(|scheme| text[from..].find(scheme).map(|i| (from + i, scheme.len()))).min() {
        let (start, scheme) = found;
        let len = text[start..].find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"')).unwrap_or(text.len() - start);
        let mut url = &text[start..start + len];
        loop {
            let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'', '*', '_', '~']);
            let unbalanced = trimmed.ends_with(')') && trimmed.matches('(').count() < trimmed.matches(')').count();
            let next = if unbalanced { &trimmed[..trimmed.len() - 1] } else { trimmed };
            if next.len() == url.len() {
                break;
            }
            url = next;
        }
        let after_boundary = start == 0 || text[..start].chars().next_back().is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '['));
        if after_boundary && url.len() > scheme {
            let at = run.start + start;
            out.push(Link { kind: LinkKind::External, embed: false, target: url.to_owned(), subpath: None, display: None, span: Span::new(at, at + url.len()) });
        }
        from = start + len.max(1);
    }
}
