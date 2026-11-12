//! Obsidian block identifiers `^name` inside a run of plain text.

use std::ops::Range;

use knotes_core::{BlockAnchor, Span};

/// An anchor is `^` at the start of the run or after whitespace, then `[A-Za-z0-9-]+`,
/// then the end of the line or of the file (checked in the source, not in the run).
pub(crate) fn scan_anchors(source: &str, run: Range<usize>, out: &mut Vec<BlockAnchor>) {
    let text = &source[run.clone()];
    for (i, _) in text.match_indices('^') {
        let at_boundary = if i == 0 {
            !source[..run.start].ends_with('\\')
        } else {
            text.as_bytes()[i - 1].is_ascii_whitespace()
        };
        if !at_boundary {
            continue;
        }
        let rest = &text.as_bytes()[i + 1..];
        let len = rest.iter().position(|&b| !(b.is_ascii_alphanumeric() || b == b'-')).unwrap_or(rest.len());
        let end = run.start + i + 1 + len;
        let at_line_end = matches!(source.as_bytes().get(end), None | Some(b'\n' | b'\r'));
        if len > 0 && at_line_end {
            let start = run.start + i;
            out.push(BlockAnchor { name: source[start + 1..end].to_owned(), span: Span::new(start, end) });
        }
    }
}
