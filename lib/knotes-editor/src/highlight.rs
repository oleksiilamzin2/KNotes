//! Styles of a note for the live preview, and the markup it hides outside the cursor line.

use std::ops::Range;

use knotes_core::{FileKind, Link, LinkKind, NoteParser};
use knotes_md::{MarkdownParser, OPTIONS};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Parser, Tag, TagEnd};

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
    /// A quote line's prefix (`> `, `> > `), with the indentation before it; `depth` counts
    /// its `>`.
    QuoteMarker { depth: u8 },
    /// A fenced code block's opening (with its language) or closing fence line.
    Fence,
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

/// A fenced code block with a language: the block, the code between its fences (with the
/// last code line's break) and the fence's info (```` ```rust ```` gives `rust`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRun {
    pub block: Range<usize>,
    pub code: Range<usize>,
    pub language: String,
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
    /// Code blocks, fences included, sorted by start.
    pub code_blocks: Vec<Range<usize>>,
    /// Fenced code blocks with a language (ADR 0025), sorted by start.
    pub code_langs: Vec<CodeRun>,
    /// Obsidian callouts, sorted by start.
    pub callouts: Vec<CalloutRun>,
    /// Image embeds alone on their line (ADR 0020), sorted by start.
    pub images: Vec<ImageRun>,
    /// Markdown tables (ADR 0021), sorted by start.
    pub tables: Vec<TableRun>,
}

/// A column's alignment, from the delimiter row (`:--`, `:-:`, `--:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// A Markdown table: its lines, with the final line break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRun {
    pub range: Range<usize>,
    /// One per column.
    pub aligns: Vec<Align>,
    /// The header row first, then the body rows; the delimiter row is not among them.
    pub rows: Vec<TableRow>,
    /// The delimiter row's line, without its line break.
    pub delimiter: Range<usize>,
}

/// A table row: its line without the line break, and each cell's text without the pipes and
/// the spaces around it (empty at the cell's place when there is none). A row may have fewer
/// cells than the table has columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub range: Range<usize>,
    pub cells: Vec<Range<usize>>,
}

/// An image or PDF embed alone on its line: `![[pic.png|300]]`, `![alt|300x200](pic.png)`,
/// `![[doc.pdf#page=3]]` (ADR 0020, 0022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRun {
    pub range: Range<usize>,
    pub link: Link,
    /// Size written after `|`, in points.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// A PDF, shown as a preview of its page `page` (from `#page=3`; the first otherwise).
    pub pdf: bool,
    pub page: Option<u32>,
}

/// An Obsidian callout: a quote whose first line starts with `[!kind]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalloutRun {
    /// The whole quote.
    pub range: Range<usize>,
    /// Lowercase, as written (`note`, `warning`, `tip`…).
    pub kind: String,
    /// `[!kind]` with its fold sign or the space after it.
    pub marker: Range<usize>,
    /// The title written after the marker; empty for the kind's own name.
    pub title: Range<usize>,
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
            .chain(self.code_blocks.iter().flat_map(|c| [c.start, c.end]))
            .chain(self.code_langs.iter().flat_map(|c| [c.block.start, c.block.end, c.code.start, c.code.end]))
            .chain(self.callouts.iter().flat_map(|c| [c.range.start, c.range.end, c.marker.start, c.marker.end, c.title.start, c.title.end]))
            .chain(self.images.iter().flat_map(|i| [i.range.start, i.range.end]))
            .chain(self.tables.iter().flat_map(|t| {
                [t.range.start, t.range.end, t.delimiter.start, t.delimiter.end]
                    .into_iter()
                    .chain(t.rows.iter().flat_map(|r| [r.range.start, r.range.end].into_iter().chain(r.cells.iter().flat_map(|c| [c.start, c.end]))))
            }))
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
            code_blocks: self.code_blocks.iter().map(|c| map(c.start)..map(c.end)).collect(),
            code_langs: self
                .code_langs
                .iter()
                .map(|c| CodeRun { block: map(c.block.start)..map(c.block.end), code: map(c.code.start)..map(c.code.end), language: c.language.clone() })
                .collect(),
            callouts: self
                .callouts
                .iter()
                .map(|c| CalloutRun {
                    range: map(c.range.start)..map(c.range.end),
                    kind: c.kind.clone(),
                    marker: map(c.marker.start)..map(c.marker.end),
                    title: map(c.title.start)..map(c.title.end),
                })
                .collect(),
            images: self.images.iter().map(|i| ImageRun { range: map(i.range.start)..map(i.range.end), ..i.clone() }).collect(),
            tables: self
                .tables
                .iter()
                .map(|t| TableRun {
                    range: map(t.range.start)..map(t.range.end),
                    aligns: t.aligns.clone(),
                    rows: t
                        .rows
                        .iter()
                        .map(|r| TableRow { range: map(r.range.start)..map(r.range.end), cells: r.cells.iter().map(|c| map(c.start)..map(c.end)).collect() })
                        .collect(),
                    delimiter: map(t.delimiter.start)..map(t.delimiter.end),
                })
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
    // Quotes open: only the outermost marks the prefixes of all its lines.
    let mut quotes = 0;
    for (event, r) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        let text = &source[r.clone()];
        match event {
            Event::Start(Tag::Table(aligns)) => {
                let aligns = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::None => Align::None,
                        Alignment::Left => Align::Left,
                        Alignment::Center => Align::Center,
                        Alignment::Right => Align::Right,
                    })
                    .collect();
                h.tables.push(TableRun { range: r.clone(), aligns, rows: Vec::new(), delimiter: r.start..r.start });
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some(table) = h.tables.last_mut() {
                    table.rows.push(TableRow { range: r.start..r.start + text.trim_end_matches(['\n', '\r']).len(), cells: Vec::new() });
                }
            }
            Event::End(TagEnd::TableHead) => {
                // The delimiter row is the line after the header's.
                if let Some(table) = h.tables.last_mut() {
                    let start = r.end;
                    let end = source[start..].find('\n').map_or(source.len(), |n| start + n);
                    table.delimiter = start..start + source[start..end].trim_end_matches('\r').len();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(row) = h.tables.last_mut().and_then(|t| t.rows.last_mut())
                    // A cell the row lacks comes at its end.
                    && r.start <= row.range.end
                {
                    let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
                    let content = text.trim_matches([' ', '\t']);
                    let start = r.start + lead.min(text.len());
                    let start = if content.is_empty() { r.start + text.len().min(1) } else { start };
                    row.cells.push(start..start + content.len());
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                if quotes == 0 {
                    quote_prefixes(&mut h, source, r.clone());
                    h.callouts.extend(callout(source, r.clone()));
                }
                quotes += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => quotes -= 1,
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
            Event::Start(Tag::CodeBlock(kind)) => {
                let block = r.start..r.start + text.trim_end().len();
                h.runs.push(Run { range: block.clone(), style: Style::CodeBlock });
                if let CodeBlockKind::Fenced(info) = kind {
                    let code = fences(&mut h, source, block.clone());
                    let language = info.trim().to_owned();
                    if !language.is_empty() {
                        h.code_langs.push(CodeRun { block: block.clone(), code, language });
                    }
                }
                h.code_blocks.push(block);
            }
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
    // A bare web address is a link with no markup: its text is the link.
    for link in h.links.iter().filter(|l| l.link.kind == LinkKind::External && !source[l.range.clone()].starts_with(['[', '<', '!'])) {
        h.runs.push(Run { range: link.range.clone(), style: Style::Link });
    }
    h.links.sort_by_key(|l| l.range.start);
    h.images = h.links.iter().filter_map(|l| image(source, l)).collect();
    h.runs.sort_by_key(|run| run.range.start);
    h.markup.sort_by_key(|m| m.start);
    h
}

/// The image or PDF `l` embeds, when it is one in the vault alone on its line.
fn image(source: &str, l: &LinkRun) -> Option<ImageRun> {
    let link = &l.link;
    let kind = FileKind::from_path(&link.target);
    if !link.embed || link.kind == LinkKind::External || !matches!(kind, FileKind::Image | FileKind::Pdf) {
        return None;
    }
    let pdf = kind == FileKind::Pdf;
    let page = link.subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok()).filter(|_| pdf);
    let line_start = source[..l.range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[l.range.end..].find('\n').map_or(source.len(), |i| l.range.end + i);
    if !source[line_start..l.range.start].trim().is_empty() || !source[l.range.end..line_end].trim().is_empty() {
        return None;
    }
    // `300` or `300x200`, after the alias's or alt text's last `|`.
    let size = link.display.as_deref().map_or("", |d| d.rsplit('|').next().unwrap_or(d)).trim();
    let (width, height) = size.split_once('x').map_or((size, ""), |(w, h)| (w, h));
    Some(ImageRun { range: l.range.clone(), link: link.clone(), width: width.parse().ok(), height: height.parse().ok(), pdf, page })
}

/// Marks the opening fence line of a fenced code block at `block`, and its closing one: its
/// last line, when that is a fence of the same character.
/// Marks a fenced block's fence lines; gives the code between them.
fn fences(h: &mut Highlight, source: &str, block: Range<usize>) -> Range<usize> {
    let text = &source[block.clone()];
    let open_end = text.find('\n').unwrap_or(text.len());
    h.runs.push(Run { range: block.start..block.start + open_end, style: Style::Fence });
    let fence = text.trim_start().as_bytes().first().copied().unwrap_or(b'`');
    let code_start = (block.start + open_end + 1).min(block.end);
    let mut code_end = block.end;
    if let Some(n) = text.rfind('\n').filter(|&n| n >= open_end) {
        let last = &text[n + 1..];
        let mark = last.trim_start();
        if mark.len() >= 3 && mark.bytes().all(|b| b == fence) {
            h.runs.push(Run { range: block.start + n + 1..block.end, style: Style::Fence });
            code_end = block.start + n + 1;
        }
    }
    code_start..code_end.max(code_start)
}

/// The callout a quote at `r` is, when its first line starts with `[!kind]` after its `>`.
fn callout(source: &str, r: Range<usize>) -> Option<CalloutRun> {
    let text = &source[r.clone()];
    let line = &text[..text.find('\n').unwrap_or(text.len())];
    let content = line.trim_start_matches(['>', ' ', '\t']);
    let at = r.start + (line.len() - content.len());
    let inner = content.strip_prefix("[!")?;
    let close = inner.find(']')?;
    let kind = &inner[..close];
    if kind.is_empty() || kind.contains(char::is_whitespace) {
        return None;
    }
    let mut end = at + 2 + close + 1;
    if matches!(source.as_bytes().get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    let line_end = at + content.len();
    end += blanks(&source[end..line_end]);
    let title = end..end + source[end..line_end].trim_end().len();
    Some(CalloutRun { range: r.start..r.start + text.trim_end().len(), kind: kind.to_lowercase(), marker: at..end, title })
}

/// Marks the prefix of each line of a quote at `r`: the first from the quote's start (it may
/// follow a list marker), the others from their line's start. Lazy lines have none.
fn quote_prefixes(h: &mut Highlight, source: &str, r: Range<usize>) {
    let mut start = r.start;
    let first_line = source[..start].rfind('\n').map_or(0, |n| n + 1);
    if source[first_line..start].bytes().all(|b| b == b' ' || b == b'\t') {
        start = first_line;
    }
    while start < r.end {
        let line_end = source[start..].find('\n').map_or(source.len(), |n| start + n);
        let (mut end, mut depth) = (start, 0u8);
        loop {
            let spaces = blanks(&source[end..line_end]);
            if source.as_bytes().get(end + spaces) != Some(&b'>') || end + spaces >= line_end {
                break;
            }
            end += spaces + 1;
            depth += 1;
            if matches!(source.as_bytes().get(end), Some(b' ' | b'\t')) && end < line_end {
                end += 1;
            }
        }
        if depth > 0 {
            h.runs.push(Run { range: start..end, style: Style::QuoteMarker { depth } });
        }
        start = line_end + 1;
    }
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
