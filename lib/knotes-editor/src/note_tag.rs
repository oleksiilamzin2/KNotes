//! A tag put into a note's frontmatter, or taken out of it and out of the note's text (ADR 0036):
//! how «Сховати від ШІ» marks a note `#no-ai`, and «Показати ШІ» takes the mark away. The rest
//! of the note stays as written; a frontmatter's `tags` keep their form (`[a, b]`, a list of
//! `- a` lines, or words), the words written as a list when one is added.

use knotes_core::NoteParser;
use knotes_md::MarkdownParser;

/// `text` with `tag` among its frontmatter's `tags` (a frontmatter made when it has none); as it
/// is when the note has the tag already, there or in its text.
pub fn with_tag(text: &str, tag: &str) -> String {
    let note = MarkdownParser::new().parse(text);
    let named = |name: &str| same_tag(name, tag);
    if note.tags.iter().any(|t| named(&t.name)) || note.frontmatter.as_ref().is_some_and(|f| f.tags.iter().any(|t| named(t))) {
        return text.to_owned();
    }
    let Some(front) = note.frontmatter else {
        return format!("---\ntags: [{tag}]\n---\n{text}");
    };
    let (start, end) = (front.span.start as usize, front.span.end as usize);
    let mut lines = Lines::of(&text[start..end]);
    match lines.key() {
        Some(at) => {
            let (key, rest) = lines.key_and_rest(at);
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                let inner = inner.trim();
                lines.set(at, format!("{key}: [{}{tag}]", if inner.is_empty() { String::new() } else { format!("{inner}, ") }));
            } else if rest.is_empty() {
                let items = lines.items_after(at);
                match items.last() {
                    Some(&last) => {
                        let indent = lines.indent(items[0]);
                        lines.insert(last + 1, format!("{indent}- {tag}"));
                    }
                    None => lines.set(at, format!("{key}: [{tag}]")),
                }
            } else {
                let mut tags = front.tags.clone();
                tags.push(tag.to_owned());
                lines.set(at, format!("{key}: [{}]", tags.join(", ")));
            }
        }
        None => {
            let closing = lines.len() - 1;
            lines.insert(closing, format!("tags: [{tag}]"));
        }
    }
    format!("{}{}{}", &text[..start], lines.join(), &text[end..])
}

/// `text` without `tag`: out of its frontmatter's `tags` (the key going with its last tag, the
/// frontmatter with its last key) and out of its text (`#tag`, a space beside it with it).
pub fn without_tag(text: &str, tag: &str) -> String {
    let note = MarkdownParser::new().parse(text);
    let front_end = note.frontmatter.as_ref().map_or(0, |f| f.span.end as usize);
    let mut out = text.to_owned();
    // In the text first, from its end: the frontmatter before it keeps its place.
    let mut inline: Vec<_> = note.tags.iter().filter(|t| same_tag(&t.name, tag) && t.span.start as usize >= front_end).map(|t| (t.span.start as usize, t.span.end as usize)).collect();
    inline.sort_by_key(|&(start, _)| std::cmp::Reverse(start));
    for (start, end) in inline {
        let (mut start, mut end) = (start, end);
        if out[end..].starts_with(' ') {
            end += 1;
        } else if out[..start].ends_with(' ') {
            start -= 1;
        }
        out.replace_range(start..end, "");
    }
    let Some(front) = note.frontmatter else { return out };
    let (start, end) = (front.span.start as usize, front.span.end as usize);
    let mut lines = Lines::of(&out[start..end]);
    if let Some(at) = lines.key() {
        let (key, rest) = lines.key_and_rest(at);
        let rest = rest.trim().to_owned();
        if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            let left: Vec<&str> = inner.split(',').map(str::trim).filter(|t| !t.is_empty() && !same_tag(t, tag)).collect();
            if left.is_empty() { lines.remove(at) } else { lines.set(at, format!("{key}: [{}]", left.join(", "))) }
        } else if rest.is_empty() {
            let items = lines.items_after(at);
            let gone: Vec<usize> = items.iter().copied().filter(|&i| same_tag(lines.item(i), tag)).collect();
            for &i in gone.iter().rev() {
                lines.remove(i);
            }
            if !gone.is_empty() && gone.len() == items.len() {
                lines.remove(at);
            }
        } else {
            let left: Vec<&str> = rest.split([' ', ',']).filter(|t| !t.is_empty() && !same_tag(t, tag)).collect();
            if left.is_empty() { lines.remove(at) } else { lines.set(at, format!("{key}: {}", left.join(" "))) }
        }
    }
    // A frontmatter with nothing left in it goes, with the line break after it.
    let block = if lines.is_empty_block() { String::new() } else { lines.join() };
    let after = if block.is_empty() { out[end..].strip_prefix('\n').unwrap_or(&out[end..]) } else { &out[end..] };
    format!("{}{block}{after}", &out[..start])
}

/// Whether `written` (in a frontmatter or after `#`, maybe quoted) names `tag` or a tag under it
/// (`no-ai/diary` is `no-ai`'s too).
fn same_tag(written: &str, tag: &str) -> bool {
    let written = written.trim().trim_matches(['"', '\'']).trim_start_matches('#');
    written.eq_ignore_ascii_case(tag) || written.get(..tag.len()).is_some_and(|head| head.eq_ignore_ascii_case(tag)) && written[tag.len()..].starts_with('/')
}

/// A frontmatter block's lines, each with its line break; the first and the last are its `---`.
struct Lines(Vec<String>);

impl Lines {
    fn of(block: &str) -> Self {
        Self(block.split_inclusive('\n').map(str::to_owned).collect())
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn join(&self) -> String {
        self.0.concat()
    }

    fn line_break(&self) -> &'static str {
        if self.0.first().is_some_and(|l| l.ends_with("\r\n")) { "\r\n" } else { "\n" }
    }

    /// The line of the `tags` (or `tag`) key, at the line's start.
    fn key(&self) -> Option<usize> {
        (1..self.0.len().saturating_sub(1)).find(|&i| {
            let line = &self.0[i];
            ["tags:", "tag:"].iter().any(|k| line.starts_with(k))
        })
    }

    fn key_and_rest(&self, at: usize) -> (String, String) {
        let line = self.0[at].trim_end_matches(['\n', '\r']);
        let (key, rest) = line.split_once(':').unwrap_or((line, ""));
        (key.to_owned(), rest.to_owned())
    }

    /// The `- item` lines right after line `at`.
    fn items_after(&self, at: usize) -> Vec<usize> {
        (at + 1..self.0.len().saturating_sub(1)).take_while(|&i| self.0[i].trim_start().starts_with('-')).collect()
    }

    /// An item line's value, after its `-`.
    fn item(&self, i: usize) -> &str {
        self.0[i].trim().trim_start_matches('-').trim()
    }

    fn indent(&self, i: usize) -> String {
        let line = &self.0[i];
        line[..line.len() - line.trim_start().len()].to_owned()
    }

    fn set(&mut self, at: usize, line: String) {
        let line_break = self.line_break();
        self.0[at] = line + line_break;
    }

    fn insert(&mut self, at: usize, line: String) {
        let line_break = self.line_break();
        self.0.insert(at, line + line_break);
    }

    fn remove(&mut self, at: usize) {
        self.0.remove(at);
    }

    /// Only its `---` lines, and blank ones.
    fn is_empty_block(&self) -> bool {
        self.0.len() <= 2 || self.0[1..self.0.len() - 1].iter().all(|l| l.trim().is_empty())
    }
}
