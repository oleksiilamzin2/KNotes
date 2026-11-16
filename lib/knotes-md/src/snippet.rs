//! Plain-text preview of a note for the note list.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::markdown_parser::OPTIONS;
use crate::{comment_ranges, mark_ranges};

/// Bytes of the note looked at: the preview is the start of the note.
const PREFIX_BYTES: usize = 4096;

/// Text of the note without frontmatter, headings, code, comments and embeds (a note's or an
/// image's), markup removed (a highlight's signs too) and whitespace collapsed, at most
/// `max_chars` characters. Links show their text.
pub fn snippet(source: &str, max_chars: usize) -> String {
    let prefix = &source[..source.floor_char_boundary(PREFIX_BYTES)];
    let dropped = comments_and_signs(prefix);
    let mut out = String::new();
    let mut skip = 0u32;
    for (event, range) in Parser::new_ext(prefix, OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Image { .. }) => skip += 1,
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::MetadataBlock(_) | TagEnd::Image) => skip -= 1,
            // Text meeting a comment or a highlight's sign: the source around them.
            Event::Text(_) if skip == 0 && dropped.iter().any(|d| d.start < range.end && range.start < d.end) => {
                let mut at = range.start;
                for d in dropped.iter().filter(|d| d.start < range.end && range.start < d.end) {
                    out.push_str(&prefix[at..d.start.max(at)]);
                    at = at.max(d.end.min(range.end));
                }
                out.push_str(&prefix[at..range.end]);
            }
            Event::Text(t) | Event::Code(t) | Event::InlineMath(t) if skip == 0 => out.push_str(&t),
            // Spaces between blocks and lines only; inline ends (links, emphasis) join directly.
            Event::SoftBreak
            | Event::HardBreak
            | Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableCell | TagEnd::BlockQuote(_))
                if skip == 0 =>
            {
                out.push(' ')
            }
            _ => {}
        }
        if out.len() > max_chars * 4 + 64 {
            break;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max_chars).collect()
}

/// The comments and the highlights' signs of `text`, sorted.
fn comments_and_signs(text: &str) -> Vec<Range<usize>> {
    if !text.contains("%%") && !text.contains("==") {
        return Vec::new();
    }
    let mut verbatim: Vec<Range<usize>> = Parser::new_ext(text, OPTIONS)
        .into_offset_iter()
        .filter(|(e, _)| matches!(e, Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_)) | Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_)))
        .map(|(_, r)| r)
        .collect();
    verbatim.sort_by_key(|r| r.start);
    let comments = comment_ranges(text, &verbatim);
    let mut skip: Vec<Range<usize>> = verbatim.iter().chain(&comments).cloned().collect();
    skip.sort_by_key(|r| r.start);
    let mut dropped: Vec<Range<usize>> = mark_ranges(text, &skip).into_iter().flat_map(|m| [m.start..m.start + 2, m.end - 2..m.end]).chain(comments).collect();
    dropped.sort_by_key(|r| r.start);
    dropped
}
