//! Styles of a note for the live preview, and the markup it hides outside the cursor line.

use std::ops::Range;

use knotes_core::NoteParser;
use knotes_md::{MarkdownParser, OPTIONS};
use pulldown_cmark::{Event, LinkType, Parser, Tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Heading(u8),
    Strong,
    Emphasis,
    Strike,
    Code,
    CodeBlock,
    Link,
    WikiLink,
    Tag,
}

/// A styled byte range of the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub range: Range<usize>,
    pub style: Style,
}

#[derive(Debug, Default)]
pub struct Highlight {
    /// Sorted by start.
    pub runs: Vec<Run>,
    /// Delimiters the live preview hides away from the cursor (`**`, `## `, `[[Note|`, `](url)`),
    /// sorted by start.
    pub markup: Vec<Range<usize>>,
}

impl Highlight {
    /// The same highlight with UTF-16 ranges (what `NSString` counts), converted in one pass.
    pub fn to_utf16(&self, source: &str) -> Highlight {
        let mut bounds: Vec<usize> =
            self.runs.iter().flat_map(|r| [r.range.start, r.range.end]).chain(self.markup.iter().flat_map(|m| [m.start, m.end])).collect();
        bounds.sort_unstable();
        bounds.dedup();
        let units = crate::utf16_offsets(source, &bounds);
        let map = |b: usize| units[bounds.binary_search(&b).expect("collected bound")];
        Highlight {
            runs: self.runs.iter().map(|r| Run { range: map(r.range.start)..map(r.range.end), style: r.style }).collect(),
            markup: self.markup.iter().map(|m| map(m.start)..map(m.end)).collect(),
        }
    }
}

pub fn highlight(source: &str) -> Highlight {
    let mut h = Highlight::default();
    for (event, r) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        let text = &source[r.clone()];
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let line_end = r.start + text.trim_end().len();
                h.runs.push(Run { range: r.start..line_end, style: Style::Heading(level as u8) });
                let hashes = text.bytes().take_while(|&b| b == b'#').count();
                if hashes > 0 {
                    let spaces = text[hashes..].bytes().take_while(|&b| b == b' ').count();
                    h.markup.push(r.start..r.start + hashes + spaces);
                }
            }
            Event::Start(Tag::Strong) => delimited(&mut h, r, Style::Strong, 2),
            Event::Start(Tag::Emphasis) => delimited(&mut h, r, Style::Emphasis, 1),
            Event::Start(Tag::Strikethrough) => {
                let n = text.bytes().take_while(|&b| b == b'~').count();
                delimited(&mut h, r, Style::Strike, n);
            }
            Event::Code(_) => {
                let n = text.bytes().take_while(|&b| b == b'`').count();
                delimited(&mut h, r, Style::Code, n);
            }
            Event::Start(Tag::CodeBlock(_)) => h.runs.push(Run { range: r.start..r.start + text.trim_end().len(), style: Style::CodeBlock }),
            Event::Start(Tag::Link { link_type: LinkType::WikiLink { has_pothole }, .. }) => {
                h.runs.push(Run { range: r.clone(), style: Style::WikiLink });
                let open = if has_pothole { text.find('|').map_or(2, |i| i + 1) } else { 2 };
                h.markup.push(r.start..r.start + open);
                h.markup.push(r.end - 2..r.end);
            }
            Event::Start(Tag::Link { link_type: LinkType::Inline, .. }) => {
                h.runs.push(Run { range: r.clone(), style: Style::Link });
                if let Some(close) = text.rfind("](") {
                    h.markup.push(r.start..r.start + 1);
                    h.markup.push(r.start + close..r.end);
                }
            }
            Event::Start(Tag::Image { link_type: LinkType::WikiLink { .. }, .. }) => {
                h.runs.push(Run { range: r.clone(), style: Style::WikiLink });
                h.markup.push(r.start..r.start + 3);
                h.markup.push(r.end - 2..r.end);
            }
            _ => {}
        }
    }
    for tag in MarkdownParser::new().parse(source).tags {
        h.runs.push(Run { range: tag.span.start as usize..tag.span.end as usize, style: Style::Tag });
    }
    h.runs.sort_by_key(|run| run.range.start);
    h.markup.sort_by_key(|m| m.start);
    h
}

/// A run whose first and last `n` bytes are its delimiters.
fn delimited(h: &mut Highlight, r: Range<usize>, style: Style, n: usize) {
    h.runs.push(Run { range: r.clone(), style });
    if n > 0 && r.len() >= 2 * n {
        h.markup.push(r.start..r.start + n);
        h.markup.push(r.end - n..r.end);
    }
}
