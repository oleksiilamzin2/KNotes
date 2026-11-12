use knotes_core::{Heading, NoteParser, ParsedNote, Span};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Obsidian-flavoured Markdown: wikilinks, math, YAML frontmatter, GFM tables, alerts.
const OPTIONS: Options = Options::ENABLE_WIKILINKS
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
    .union(Options::ENABLE_TABLES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_GFM);

#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }
}

impl NoteParser for MarkdownParser {
    fn parse(&self, source: &str) -> ParsedNote {
        let mut note = ParsedNote::default();
        let mut heading: Option<Heading> = None;

        for (event, range) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    heading = Some(Heading {
                        level: level as u8,
                        text: String::new(),
                        span: Span::new(range.start, range.end),
                    });
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(mut h) = heading.take() {
                        let trimmed = h.text.trim_end().len();
                        h.text.truncate(trimmed);
                        note.headings.push(h);
                    }
                }
                Event::Text(t) | Event::Code(t) | Event::InlineMath(t) => {
                    if let Some(h) = heading.as_mut() {
                        h.text.push_str(&t);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(h) = heading.as_mut() {
                        h.text.push(' ');
                    }
                }
                _ => {}
            }
        }
        note
    }
}
