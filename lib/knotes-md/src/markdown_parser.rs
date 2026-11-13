use std::ops::Range;

use knotes_core::{Heading, Link, LinkKind, NoteParser, ParsedNote, Span};
use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::anchor_scan::scan_anchors;
use crate::chunking::{TopBlock, chunk};
use crate::frontmatter::parse_frontmatter;
use crate::link_target::{has_scheme, percent_decode, split_subpath};
use crate::snippet::snippet;
use crate::tag_scan::scan_tags;
use crate::url_scan::scan_urls;

/// Obsidian-flavoured Markdown: wikilinks, math, YAML frontmatter, GFM tables, alerts.
pub const OPTIONS: Options = Options::ENABLE_WIKILINKS
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
    .union(Options::ENABLE_TABLES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_GFM);

/// Characters of the note-list preview.
const SNIPPET_CHARS: usize = 200;

/// Default block size: a few hundred tokens of Ukrainian or English text.
pub const DEFAULT_MAX_BLOCK_BYTES: usize = 2048;

#[derive(Debug, Clone, Copy)]
pub struct MarkdownParser {
    max_block_bytes: usize,
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self::with_max_block_bytes(DEFAULT_MAX_BLOCK_BYTES)
    }

    pub fn with_max_block_bytes(max_block_bytes: usize) -> Self {
        Self { max_block_bytes: max_block_bytes.max(1) }
    }
}

impl NoteParser for MarkdownParser {
    fn parse(&self, source: &str) -> ParsedNote {
        let mut c = Collector { source, ..Collector::default() };
        for (event, range) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
            if !matches!(event, Event::Text(_)) {
                c.flush_run();
            }
            c.track_top_block(&event, &range);
            let span = Span::new(range.start, range.end);
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    c.heading = Some(Heading { level: level as u8, text: String::new(), span });
                }
                Event::End(TagEnd::Heading(_)) => c.end_heading(),
                Event::Start(Tag::Link { link_type, dest_url, .. }) => c.start_link(link_type, dest_url, false, span),
                Event::Start(Tag::Image { link_type, dest_url, .. }) => c.start_link(link_type, dest_url, true, span),
                Event::End(TagEnd::Link | TagEnd::Image) => c.end_link(),
                Event::Start(Tag::CodeBlock(_)) => c.in_verbatim = true,
                Event::End(TagEnd::CodeBlock) => c.in_verbatim = false,
                Event::Start(Tag::MetadataBlock(_)) => {
                    c.in_verbatim = true;
                    c.yaml = Some((span, String::new()));
                }
                Event::End(TagEnd::MetadataBlock(_)) => c.end_frontmatter(),
                Event::Text(t) => {
                    if let Some((_, yaml)) = c.yaml.as_mut() {
                        yaml.push_str(&t);
                    }
                    c.text(&t);
                    c.plain_text(range);
                }
                Event::Code(t) | Event::InlineMath(t) => c.text(&t),
                Event::SoftBreak | Event::HardBreak => c.text(" "),
                _ => {}
            }
        }
        c.flush_run();
        chunk(source, &c.top, self.max_block_bytes, &mut c.note.blocks);
        c.note.snippet = snippet(source, SNIPPET_CHARS);
        c.note
    }
}

/// Single-pass state over the pulldown-cmark event stream.
#[derive(Default)]
struct Collector<'s> {
    source: &'s str,
    note: ParsedNote,
    heading: Option<Heading>,
    /// Open links: index into `note.links` and whether their text is the display text.
    open_links: Vec<(usize, bool)>,
    /// Inside a code block or frontmatter: text there is not prose.
    in_verbatim: bool,
    /// Contiguous run of prose; pulldown-cmark splits text at `_`, `*`, `[` and similar.
    run: Option<Range<usize>>,
    /// Frontmatter being read: block span and YAML text.
    yaml: Option<(Span, String)>,
    /// Nesting depth of Start/End events; 0 means between top-level blocks.
    depth: u32,
    /// Top-level blocks (frontmatter excluded), input for chunking.
    top: Vec<TopBlock>,
}

impl Collector<'_> {
    fn track_top_block(&mut self, event: &Event, range: &Range<usize>) {
        match event {
            Event::Start(_) => self.depth += 1,
            Event::End(end) => {
                self.depth -= 1;
                if self.depth == 0 && !matches!(end, TagEnd::MetadataBlock(_)) {
                    // The heading is pushed right after this call, so its index is the current length.
                    let heading = matches!(end, TagEnd::Heading(_)).then_some(self.note.headings.len() as u32);
                    self.top.push(TopBlock { range: range.clone(), heading });
                }
            }
            // Leaf blocks such as a thematic break.
            _ if self.depth == 0 => self.top.push(TopBlock { range: range.clone(), heading: None }),
            _ => {}
        }
    }

    fn end_frontmatter(&mut self) {
        self.in_verbatim = false;
        if let Some((span, yaml)) = self.yaml.take() {
            self.note.frontmatter = Some(parse_frontmatter(&yaml, span));
        }
    }

    fn text(&mut self, t: &str) {
        if let Some(h) = self.heading.as_mut() {
            h.text.push_str(t);
        }
        if let Some(&(i, true)) = self.open_links.last() {
            self.note.links[i].display.get_or_insert_with(String::new).push_str(t);
        }
    }

    /// Extends the prose run with a text event, unless the text is code, frontmatter or link text.
    fn plain_text(&mut self, range: Range<usize>) {
        if self.in_verbatim || !self.open_links.is_empty() {
            self.flush_run();
            return;
        }
        match self.run.as_mut() {
            Some(run) if run.end == range.start => run.end = range.end,
            _ => {
                self.flush_run();
                self.run = Some(range);
            }
        }
    }

    fn flush_run(&mut self) {
        if let Some(run) = self.run.take() {
            scan_tags(self.source, run.clone(), &mut self.note.tags);
            scan_urls(self.source, run.clone(), &mut self.note.links);
            scan_anchors(self.source, run, &mut self.note.anchors);
        }
    }

    fn end_heading(&mut self) {
        if let Some(mut h) = self.heading.take() {
            let trimmed = h.text.trim_end().len();
            h.text.truncate(trimmed);
            self.note.headings.push(h);
        }
    }

    fn start_link(&mut self, link_type: LinkType, dest: CowStr, embed: bool, span: Span) {
        let (kind, collect_display) = match link_type {
            LinkType::WikiLink { has_pothole } => (LinkKind::Wiki, has_pothole),
            LinkType::Autolink | LinkType::Email => (LinkKind::External, false),
            _ if has_scheme(&dest) => (LinkKind::External, true),
            _ => (LinkKind::Markdown, true),
        };
        let (target, subpath) = match kind {
            LinkKind::External => (dest.into_string(), None),
            LinkKind::Wiki => {
                // `[[Note\|alias]]` inside a table leaves `\` at the end; Obsidian forbids `\` in names.
                let dest = dest.strip_suffix('\\').unwrap_or(&dest);
                let (path, sub) = split_subpath(dest);
                (path.to_owned(), sub.map(str::to_owned))
            }
            LinkKind::Markdown => {
                let (path, sub) = split_subpath(&dest);
                (percent_decode(path).into_owned(), sub.map(|s| percent_decode(s).into_owned()))
            }
        };
        self.open_links.push((self.note.links.len(), collect_display));
        self.note.links.push(Link { kind, embed, target, subpath, display: None, span });
    }

    fn end_link(&mut self) {
        if let Some((i, _)) = self.open_links.pop() {
            let link = &mut self.note.links[i];
            if link.display.as_deref().is_some_and(str::is_empty) {
                link.display = None;
            }
        }
    }
}
