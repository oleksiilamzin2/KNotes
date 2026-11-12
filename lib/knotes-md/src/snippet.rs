//! Plain-text preview of a note for the note list.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::markdown_parser::OPTIONS;

/// Bytes of the note looked at: the preview is the start of the note.
const PREFIX_BYTES: usize = 4096;

/// Text of the note without frontmatter, headings and code, markup removed and whitespace
/// collapsed, at most `max_chars` characters. Links show their text.
pub fn snippet(source: &str, max_chars: usize) -> String {
    let prefix = &source[..source.floor_char_boundary(PREFIX_BYTES)];
    let mut out = String::new();
    let mut skip = 0u32;
    for event in Parser::new_ext(prefix, OPTIONS) {
        match event {
            Event::Start(Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::MetadataBlock(_)) => skip += 1,
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => skip -= 1,
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
