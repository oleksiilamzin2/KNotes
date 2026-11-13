//! Styles of a note for the live preview, and the markup it hides outside the cursor line.

use std::ops::Range;

use knotes_core::{Link, NoteParser};
use knotes_md::{MarkdownParser, OPTIONS};
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// A formula, `$…$` or `$$…$$` with its dollars.
    Math,
    /// A list item's marker with the space after it (`- `, `1. `, a task's `- [ ] `), without
    /// the indentation before it; `depth` 0 for a top-level item.
    ListMarker { kind: ListKind, depth: u8 },
    /// The text of a done task, on its marker's line.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    Bullet,
    Ordered,
    Task { checked: bool },
}

/// A styled byte range of the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub range: Range<usize>,
    pub style: Style,
}

/// A formula: its source range with the dollars, the LaTeX inside, display or inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaRun {
    pub range: Range<usize>,
    pub tex: String,
    pub display: bool,
}

/// A link and the source range it covers, brackets and destination included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRun {
    pub range: Range<usize>,
    pub link: Link,
}

#[derive(Debug, Default)]
pub struct Highlight {
    /// Sorted by start.
    pub runs: Vec<Run>,
    /// Delimiters the live preview hides away from the cursor (`**`, `## `, `[[Note|`, `](url)`),
    /// sorted by start.
    pub markup: Vec<Range<usize>>,
    /// Links with their targets, sorted by start.
    pub links: Vec<LinkRun>,
    /// Formulas, sorted by start.
    pub formulas: Vec<FormulaRun>,
}

impl Highlight {
    /// Style of the innermost run around each markup range (the last one in `runs` order that
    /// contains it), in one pass: a stack of runs open at the current position.
    pub fn markup_owners(&self) -> Vec<Option<Style>> {
        let mut open: Vec<&Run> = Vec::new();
        let mut next = 0;
        self.markup
            .iter()
            .map(|m| {
                while let Some(run) = self.runs.get(next).filter(|r| r.range.start <= m.start) {
                    open.push(run);
                    next += 1;
                }
                // Runs ending before this markup cannot hold any later one either.
                open.retain(|r| r.range.end >= m.start);
                open.iter().rev().find(|r| r.range.start <= m.start && m.end <= r.range.end).map(|r| r.style)
            })
            .collect()
    }

    /// The same highlight with UTF-16 ranges (what `NSString` counts), converted in one pass.
    pub fn to_utf16(&self, source: &str) -> Highlight {
        let mut bounds: Vec<usize> = self
            .runs
            .iter()
            .flat_map(|r| [r.range.start, r.range.end])
            .chain(self.markup.iter().flat_map(|m| [m.start, m.end]))
            .chain(self.links.iter().flat_map(|l| [l.range.start, l.range.end]))
            .chain(self.formulas.iter().flat_map(|f| [f.range.start, f.range.end]))
            .collect();
        bounds.sort_unstable();
        bounds.dedup();
        let units = crate::utf16_offsets(source, &bounds);
        let map = |b: usize| units[bounds.binary_search(&b).expect("collected bound")];
        Highlight {
            runs: self.runs.iter().map(|r| Run { range: map(r.range.start)..map(r.range.end), style: r.style }).collect(),
            markup: self.markup.iter().map(|m| map(m.start)..map(m.end)).collect(),
            links: self.links.iter().map(|l| LinkRun { range: map(l.range.start)..map(l.range.end), link: l.link.clone() }).collect(),
            formulas: self
                .formulas
                .iter()
                .map(|f| FormulaRun { range: map(f.range.start)..map(f.range.end), tex: f.tex.clone(), display: f.display })
                .collect(),
        }
    }
}

pub fn highlight(source: &str) -> Highlight {
    let mut h = Highlight::default();
    // Open lists, ordered or not, and the marker run of the item just started (a task marker
    // follows it).
    let mut lists: Vec<bool> = Vec::new();
    let mut item: Option<usize> = None;
    for (event, r) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        let text = &source[r.clone()];
        match event {
            Event::Start(Tag::List(start)) => lists.push(start.is_some()),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let ordered = lists.last().copied().unwrap_or(false);
                // A nested item indented with tabs starts at the line break before it.
                let start = r.start + (text.len() - text.trim_start().len());
                let text = &source[start..r.end];
                let marker = if ordered { text.bytes().take_while(u8::is_ascii_digit).count() + 1 } else { 1 }.min(text.len());
                let end = start + marker + blanks(&text[marker..]);
                let kind = if ordered { ListKind::Ordered } else { ListKind::Bullet };
                h.runs.push(Run { range: start..end, style: Style::ListMarker { kind, depth: lists.len().saturating_sub(1) as u8 } });
                item = Some(h.runs.len() - 1);
            }
            Event::TaskListMarker(checked) => {
                if let Some(i) = item.take()
                    && let Style::ListMarker { depth, .. } = h.runs[i].style
                {
                    let end = r.end + blanks(&source[r.end..]);
                    h.runs[i] = Run { range: h.runs[i].range.start..end, style: Style::ListMarker { kind: ListKind::Task { checked }, depth } };
                    let line = &source[end..source[end..].find('\n').map_or(source.len(), |n| end + n)];
                    if checked && !line.trim_end().is_empty() {
                        h.runs.push(Run { range: end..end + line.trim_end().len(), style: Style::Done });
                    }
                }
            }
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
            Event::InlineMath(tex) => formula(&mut h, r, &tex, false),
            Event::DisplayMath(tex) => formula(&mut h, r, &tex, true),
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
    let parsed = MarkdownParser::new().parse(source);
    for tag in parsed.tags {
        h.runs.push(Run { range: tag.span.start as usize..tag.span.end as usize, style: Style::Tag });
    }
    h.links = parsed.links.into_iter().map(|link| LinkRun { range: link.span.start as usize..link.span.end as usize, link }).collect();
    h.links.sort_by_key(|l| l.range.start);
    h.runs.sort_by_key(|run| run.range.start);
    h.markup.sort_by_key(|m| m.start);
    h
}

/// Length of the spaces and tabs `text` starts with.
fn blanks(text: &str) -> usize {
    text.bytes().take_while(|&b| b == b' ' || b == b'\t').count()
}

fn formula(h: &mut Highlight, r: Range<usize>, tex: &str, display: bool) {
    h.runs.push(Run { range: r.clone(), style: Style::Math });
    h.formulas.push(FormulaRun { range: r, tex: tex.trim().to_owned(), display });
}

/// A run whose first and last `n` bytes are its delimiters.
fn delimited(h: &mut Highlight, r: Range<usize>, style: Style, n: usize) {
    h.runs.push(Run { range: r.clone(), style });
    if n > 0 && r.len() >= 2 * n {
        h.markup.push(r.start..r.start + n);
        h.markup.push(r.end - n..r.end);
    }
}
