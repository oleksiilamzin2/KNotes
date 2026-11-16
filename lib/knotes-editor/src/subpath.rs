//! Where a link's `#Heading` or `#^block` points inside a note (ADR 0013).

use std::ops::Range;

use knotes_core::{Heading, NoteParser};
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

/// Headings of a note, in source order (for `[[note#` completion).
pub fn note_headings(source: &str) -> Vec<Heading> {
    MarkdownParser::new().parse(source).headings
}

/// The part of a note an embed shows (ADR 0034): the whole note without its frontmatter; the
/// section of heading `Heading` (case ignored), its heading line to the next heading as high or
/// higher; or block `^id`: its paragraph (a list item alone), the anchor left out. Trailing
/// blank lines are left out; `None` when the heading or block is not there.
pub fn embedded_part(source: &str, subpath: Option<&str>) -> Option<Range<usize>> {
    let note = MarkdownParser::new().parse(source);
    let (start, end) = match subpath.map(|s| s.strip_prefix('^').ok_or(s)) {
        None => {
            let start = note.frontmatter.as_ref().map_or(0, |f| f.span.end as usize);
            (start + (source.len() - start - source[start..].trim_start_matches(['\n', '\r']).len()), source.len())
        }
        Some(Ok(id)) => {
            let anchor = note.anchors.iter().find(|a| a.name == id)?;
            let at = anchor.span.start as usize;
            let line = source[..at].rfind('\n').map_or(0, |i| i + 1);
            let item = source[line..].trim_start().starts_with(['-', '*', '+']) || source[line..].trim_start().starts_with(|c: char| c.is_ascii_digit());
            let start = if item { line } else { source[..at].rfind("\n\n").map_or(0, |i| i + 2) };
            (start, at)
        }
        Some(Err(heading)) => {
            let wanted = heading.trim().to_lowercase();
            let i = note.headings.iter().position(|h| h.text.trim().to_lowercase() == wanted)?;
            let level = note.headings[i].level;
            let end = note.headings[i + 1..].iter().find(|h| h.level <= level).map_or(source.len(), |h| h.span.start as usize);
            (note.headings[i].span.start as usize, end)
        }
    };
    let end = start + source[start..end].trim_end().len();
    Some(start..end)
}
