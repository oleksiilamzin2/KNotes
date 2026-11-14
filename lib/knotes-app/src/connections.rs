//! The connections of the document open, for the right panel (ADR 0026): the notes linking to
//! it and the lines they do it on, the notes naming it without a link, and its own links.

use std::ops::Range;

use knotes_core::{LinkKind, LinkResolver, ParsedNote};

/// Characters of a line kept on each side of a mention when the line is long.
const CONTEXT: usize = 70;

/// A place in a note that links to the document (or names it): where in the note's text
/// (bytes), the line it is on (trimmed, cut around it when long) and where in that line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub range: Range<usize>,
    pub line: String,
    pub mark: Range<usize>,
}

/// One of the document's links' targets: as written (or the link's text), the file it
/// resolves to (`None`: not there yet) and the links to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub label: String,
    pub resolved: Option<String>,
    pub mentions: Vec<Mention>,
}

/// The links in the note `source` (its `text`, parsed) that resolve to `target`.
pub fn backlink_mentions(target: &str, source: &str, text: &str, parsed: &ParsedNote, resolver: &dyn LinkResolver) -> Vec<Mention> {
    parsed
        .links
        .iter()
        .filter(|l| l.kind != LinkKind::External && resolver.resolve(l.kind, &l.target, source).as_deref() == Some(target))
        .map(|l| mention(text, l.span.start as usize..l.span.end as usize))
        .collect()
}

/// Where `name` stands as a word in `text`, ignoring case, outside its links.
pub fn unlinked_mentions(name: &str, text: &str, parsed: &ParsedNote) -> Vec<Mention> {
    let name: Vec<char> = name.to_lowercase().chars().collect();
    if name.is_empty() {
        return Vec::new();
    }
    let in_link = |at: usize| parsed.links.iter().any(|l| (l.span.start as usize..l.span.end as usize).contains(&at));
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let lower: Vec<char> = chars.iter().map(|&(_, c)| c.to_lowercase().next().unwrap_or(c)).collect();
    let word = |i: Option<&char>| i.is_some_and(|c| c.is_alphanumeric());
    let mut found = Vec::new();
    let mut i = 0;
    while i + name.len() <= lower.len() {
        let fits = lower[i..i + name.len()] == name[..] && !word(i.checked_sub(1).and_then(|p| lower.get(p))) && !word(lower.get(i + name.len()));
        if fits && !in_link(chars[i].0) {
            let end = chars.get(i + name.len()).map_or(text.len(), |&(at, _)| at);
            found.push(mention(text, chars[i].0..end));
            i += name.len();
        } else {
            i += 1;
        }
    }
    found
}

/// The note's links to other vault files (web links left out), one entry per target in the
/// order they first appear.
pub fn outgoing_links(source: &str, text: &str, parsed: &ParsedNote, resolver: &dyn LinkResolver) -> Vec<Outgoing> {
    let mut out: Vec<Outgoing> = Vec::new();
    for link in parsed.links.iter().filter(|l| l.kind != LinkKind::External && !l.target.is_empty()) {
        let resolved = resolver.resolve(link.kind, &link.target, source);
        let at = mention(text, link.span.start as usize..link.span.end as usize);
        let same = |o: &Outgoing| match (&o.resolved, &resolved) {
            (Some(a), Some(b)) => a == b,
            (None, None) => o.label.eq_ignore_ascii_case(&link.target),
            _ => false,
        };
        match out.iter_mut().find(|o| same(o)) {
            Some(o) => o.mentions.push(at),
            None => out.push(Outgoing { label: link.target.clone(), resolved, mentions: vec![at] }),
        }
    }
    out
}

/// `text` with the mention at `range` made a wikilink to `target`, its words kept as the
/// link's text when they differ from the target.
pub fn make_link(text: &str, range: Range<usize>, target: &str) -> String {
    let words = &text[range.clone()];
    let link = if words == target { format!("[[{target}]]") } else { format!("[[{target}|{words}]]") };
    format!("{}{link}{}", &text[..range.start], &text[range.end..])
}

/// The mention at `range` with its line.
fn mention(text: &str, range: Range<usize>) -> Mention {
    let start = text[..range.start].rfind('\n').map_or(0, |n| n + 1);
    let end = text[range.end..].find('\n').map_or(text.len(), |n| range.end + n);
    let line = &text[start..end];
    let lead = line.len() - line.trim_start().len();
    let (mut from, mut to) = (start + lead, end - (line.len() - line.trim_end().len()));
    // A long line: the mention with some words on each side.
    let before = text[from..range.start].chars().count();
    let after = text[range.end..to.max(range.end)].chars().count();
    let cut_start = before > CONTEXT;
    let cut_end = after > CONTEXT;
    if cut_start {
        from = text[..range.start].char_indices().rev().nth(CONTEXT - 1).map_or(from, |(at, _)| at);
    }
    if cut_end {
        to = text[range.end..].char_indices().nth(CONTEXT).map_or(to, |(at, _)| range.end + at);
    }
    let prefix = if cut_start { "…" } else { "" };
    let line = format!("{prefix}{}{}", &text[from..to], if cut_end { "…" } else { "" });
    let mark = prefix.len() + range.start - from..prefix.len() + range.end - from;
    Mention { range, line, mark }
}
