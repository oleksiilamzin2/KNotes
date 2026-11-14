//! A model's reply (Markdown) as runs of styled text, for the panel to set.

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub code_block: bool,
    /// 1–6 in a heading, 0 outside.
    pub heading: u8,
    pub quote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunLink {
    /// A note or PDF, as the link names it (`Projects/Radar`, `budget.pdf#page=3`).
    Note(String),
    Web(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
    pub link: Option<RunLink>,
}

pub fn reply_runs(markdown: &str) -> Vec<Run> {
    let mut out = Runs::default();
    let (mut bold, mut italic) = (0, 0);
    let mut lists: Vec<Option<u64>> = Vec::new();
    for event in Parser::new_ext(markdown, Options::ENABLE_WIKILINKS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS) {
        match event {
            Event::Start(Tag::Paragraph) => out.block(),
            Event::End(TagEnd::Paragraph) => out.end_block(if lists.is_empty() { 2 } else { 1 }),
            Event::Start(Tag::Heading { level, .. }) => {
                out.block();
                out.style.heading = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                out.style.heading = 0;
                out.end_block(2);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                out.block();
                out.style.code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                out.style.code_block = false;
                out.trim_end_newline();
                out.end_block(2);
            }
            Event::Start(Tag::BlockQuote(_)) => out.style.quote = true,
            Event::End(TagEnd::BlockQuote(_)) => out.style.quote = false,
            Event::Start(Tag::List(first)) => {
                out.block();
                lists.push(first);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
                out.end_block(if lists.is_empty() { 2 } else { 1 });
            }
            Event::Start(Tag::Item) => {
                out.block();
                let depth = lists.len().saturating_sub(1);
                let marker = match lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => "• ".to_string(),
                };
                out.push(&format!("{}{marker}", "    ".repeat(depth)), RunStyle::default(), None);
                out.in_item = true;
            }
            Event::End(TagEnd::Item) => out.end_block(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::Start(Tag::Link { link_type, dest_url, .. }) => {
                let url = dest_url.to_string();
                out.link = Some(if matches!(link_type, LinkType::WikiLink { .. }) || !url.contains("://") && !url.starts_with("mailto:") { RunLink::Note(url) } else { RunLink::Web(url) });
            }
            Event::End(TagEnd::Link) => out.link = None,
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                let style = RunStyle { bold: bold > 0, italic: italic > 0, ..out.style };
                out.push(&text, style, out.link.clone());
            }
            Event::Code(text) => {
                let style = RunStyle { code: true, bold: bold > 0, italic: italic > 0, ..out.style };
                out.push(&text, style, out.link.clone());
            }
            Event::SoftBreak => out.push(" ", out.style, out.link.clone()),
            Event::HardBreak => out.push("\n", out.style, None),
            Event::Rule => {
                out.block();
                out.push("———", RunStyle::default(), None);
                out.end_block(2);
            }
            Event::TaskListMarker(done) => out.push(if done { "☑ " } else { "☐ " }, RunStyle::default(), None),
            _ => {}
        }
    }
    out.runs
}

#[derive(Default)]
struct Runs {
    runs: Vec<Run>,
    style: RunStyle,
    link: Option<RunLink>,
    /// Line breaks owed before the next block.
    breaks: usize,
    /// Just after a list item's marker: its first block starts on the same line.
    in_item: bool,
}

impl Runs {
    fn block(&mut self) {
        if self.in_item {
            self.in_item = false;
            self.breaks = 0;
            return;
        }
        if !self.runs.is_empty() && self.breaks > 0 {
            let breaks = "\n".repeat(self.breaks);
            self.push(&breaks, RunStyle::default(), None);
        }
        self.breaks = 0;
    }

    fn end_block(&mut self, breaks: usize) {
        self.in_item = false;
        self.breaks = self.breaks.max(breaks);
    }

    fn trim_end_newline(&mut self) {
        if let Some(last) = self.runs.last_mut()
            && last.text.ends_with('\n')
        {
            last.text.pop();
        }
    }

    fn push(&mut self, text: &str, style: RunStyle, link: Option<RunLink>) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut()
            && last.style == style
            && last.link == link
        {
            last.text.push_str(text);
            return;
        }
        self.runs.push(Run { text: text.to_string(), style, link });
    }
}
