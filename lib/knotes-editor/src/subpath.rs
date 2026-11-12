//! Where a link's `#Heading` or `#^block` points inside a note (ADR 0013).

use knotes_core::NoteParser;
use knotes_md::MarkdownParser;

/// Byte offset of the heading named `subpath` (case-insensitive), or of the start of the line
/// holding block `^id` when `subpath` starts with `^`.
pub fn subpath_offset(source: &str, subpath: &str) -> Option<usize> {
    let note = MarkdownParser::new().parse(source);
    match subpath.strip_prefix('^') {
        Some(id) => note.anchors.iter().find(|a| a.name == id).map(|a| {
            let at = a.span.start as usize;
            source[..at].rfind('\n').map_or(0, |i| i + 1)
        }),
        None => {
            let wanted = subpath.trim().to_lowercase();
            note.headings.iter().find(|h| h.text.trim().to_lowercase() == wanted).map(|h| h.span.start as usize)
        }
    }
}
