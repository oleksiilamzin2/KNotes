//! A note's title, its first line (ADR 0019): the note's name, so the tags and links written
//! in it are not the note's.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::markdown_parser::OPTIONS;
use crate::tag_scan::scan_tags;

/// Byte range of the note's title: the first line of its first block when that block is a
/// heading or a paragraph (after the frontmatter), unless the line holds nothing but tags and
/// links — a line of tags names nothing and its tags stay the note's.
pub fn title_range(source: &str) -> Option<Range<usize>> {
    for (event, range) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        let Event::Start(tag) = event else { continue };
        if matches!(tag, Tag::MetadataBlock(_)) {
            continue;
        }
        if !matches!(tag, Tag::Heading { .. } | Tag::Paragraph) {
            return None;
        }
        let text = &source[range.clone()];
        let line = text.find('\n').map_or(text, |n| &text[..n]).trim_end();
        return (!words(line, false).is_empty()).then(|| range.start..range.start + line.len());
    }
    None
}

/// The words a title line shows, as a name: markup and tags dropped, links reduced to the
/// text they show, embeds left out, spaces collapsed.
pub fn plain_title(line: &str) -> String {
    words(line, true)
}

/// The words of `line` without markup or tags; the text of links only with `links`.
fn words(line: &str, links: bool) -> String {
    let mut shown = String::new();
    let (mut embeds, mut in_link) = (0, 0);
    for event in Parser::new_ext(line, OPTIONS) {
        match event {
            Event::Start(Tag::Image { .. }) => embeds += 1,
            Event::End(TagEnd::Image) => embeds -= 1,
            Event::Start(Tag::Link { .. }) => in_link += 1,
            Event::End(TagEnd::Link) => in_link -= 1,
            Event::Text(t) | Event::Code(t) | Event::InlineMath(t) if embeds == 0 && (links || in_link == 0) => shown.push_str(&t),
            Event::SoftBreak | Event::HardBreak => shown.push(' '),
            _ => {}
        }
    }
    let mut tags = Vec::new();
    scan_tags(&shown, 0..shown.len(), &mut tags);
    for tag in tags.iter().rev() {
        shown.replace_range(tag.span.start as usize..tag.span.end as usize, " ");
    }
    shown.split_whitespace().collect::<Vec<_>>().join(" ")
}
