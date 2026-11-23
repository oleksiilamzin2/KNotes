//! What the completion offers while a link or a tag is typed (ADR 0013, 0033): the rows of its
//! list, each with what it puts in the note.

use knotes_core::{BlockAnchor, Heading, LinkResolver, NoteRow};

use crate::{TagNode, has_nested_tags, suggest_tags};

/// A row of the completion's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub title: String,
    /// Folder of a note, empty for the vault root; a heading's level; a tag's notes; the start
    /// of a block's line.
    pub detail: String,
    /// Text that replaces what was typed after `[[` (or after `#`).
    pub insert: String,
}

/// The `n` notes changed last, the latest first: offered before anything is typed after `[[`.
pub fn recent_notes(rows: &[NoteRow], n: usize) -> Vec<String> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let n = n.min(order.len());
    if n == 0 {
        return Vec::new();
    }
    order.select_nth_unstable_by_key(n - 1, |&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.truncate(n);
    order.sort_by_key(|&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.into_iter().map(|i| rows[i].path.clone()).collect()
}

/// The notes offered for a link typed in the note `source`: each of `found` (its path, and the
/// alias it was found by) under its name or that alias, with its folder, and the shortest link
/// that reaches it from there (`[[link|alias]]` when found by an alias).
pub fn offer_notes(found: Vec<(String, Option<String>)>, source: &str, links: &dyn LinkResolver) -> Vec<Offered> {
    found
        .into_iter()
        .map(|(path, alias)| {
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
            let name = name.strip_suffix(".md").unwrap_or(name);
            let link = links.link_text(&path, source);
            Offered { title: alias.clone().unwrap_or_else(|| name.to_owned()), detail: folder.to_owned(), insert: alias.map_or_else(|| link.clone(), |alias| format!("{link}|{alias}")) }
        })
        .collect()
}

/// The headings offered after `[[Note#`: those holding `query` (any case), in the note's
/// order, `most` at most, each with its level.
pub fn offer_headings(headings: Vec<Heading>, query: &str, most: usize) -> Vec<Offered> {
    let query = query.to_lowercase();
    headings.into_iter().filter(|h| h.text.to_lowercase().contains(&query)).take(most).map(|h| Offered { insert: h.text.clone(), title: h.text, detail: format!("H{}", h.level) }).collect()
}

/// The blocks offered after `[[Note#^`: the anchors of the note's `text` holding `query`, each
/// with the start of its line.
pub fn offer_blocks(text: &str, anchors: Vec<BlockAnchor>, query: &str, most: usize) -> Vec<Offered> {
    let query = query.to_lowercase();
    anchors
        .into_iter()
        .filter(|a| a.name.to_lowercase().contains(&query))
        .take(most)
        .map(|a| {
            let at = (a.span.start as usize).min(text.len());
            let start = text[..at].rfind('\n').map_or(0, |n| n + 1);
            let line: String = text[start..at].trim().chars().take(60).collect();
            Offered { title: format!("^{}", a.name), detail: line, insert: format!("^{}", a.name) }
        })
        .collect()
}

/// The tags offered for the one typed (`suggest_tags`), each with its notes, and `›` after
/// them for one with tags nested under it (Tab steps into it): shown with `#` in a note's
/// text, bare in its frontmatter.
pub fn offer_tags(query: &str, frontmatter: bool, space: &[TagNode], all: &[TagNode], most: usize) -> Vec<Offered> {
    suggest_tags(query, space, all, most)
        .into_iter()
        .map(|(name, notes)| {
            let nested = has_nested_tags(all, name) || has_nested_tags(space, name);
            Offered { title: if frontmatter { name.to_owned() } else { format!("#{name}") }, detail: format!("{notes}{}", if nested { " ›" } else { "" }), insert: name.to_owned() }
        })
        .collect()
}

/// The find after (`forward`) or before the one shown, of `count`, round their ends; the first
/// when none is shown; none when there are none.
pub fn step_found(shown: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match shown {
        Some(i) if forward => (i + 1) % count,
        Some(i) => (i + count - 1) % count,
        None => 0,
    })
}
