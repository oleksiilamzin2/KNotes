//! Where a link leads: a note at a heading, a PDF at a page, the web, or a note still to make.
//! The window opens what this says; nothing is opened or written here.

use knotes_core::{FileKind, Link, LinkKind, LinkResolver, Span};

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followed {
    /// The web's address (or any link with a scheme), for its own app.
    Web(String),
    /// A PDF of the vault, at the page the link names (`#page=3`).
    Pdf { path: String, page: Option<u32> },
    /// A note of the vault, at the heading or block the link names.
    Note { path: String, subpath: Option<String> },
    /// A note the vault has not: a note's own link makes it (`linked_note_path`), a link from
    /// anywhere else opens nothing.
    Missing,
    /// Nothing to follow yet: the links of the vault are not known (before its first snapshot).
    Nothing,
}

/// Where `link`, written in the note `from`, leads; `links` resolves it (none before the first
/// snapshot).
pub fn follow_link<L: LinkResolver + ?Sized>(link: &Link, from: &str, links: Option<&L>) -> Followed {
    if link.kind == LinkKind::External {
        return Followed::Web(link.target.clone());
    }
    let Some(links) = links else { return Followed::Nothing };
    match links.resolve(link.kind, &link.target, from) {
        None => Followed::Missing,
        Some(path) if FileKind::from_path(&path) == FileKind::Pdf => {
            let page = link.subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok());
            Followed::Pdf { path, page }
        }
        Some(path) => Followed::Note { path, subpath: link.subpath.clone() },
    }
}

/// A link as a chat's answer or a card's text names it (`Projects/Radar`, `budget.pdf#page=3`):
/// a wikilink to that target.
pub fn named_link(target: &str) -> Link {
    let (path, subpath) = match target.split_once('#') {
        Some((path, subpath)) => (path, Some(subpath.to_owned())),
        None => (target, None),
    };
    Link { kind: LinkKind::Wiki, embed: false, target: path.to_owned(), subpath, display: None, span: Span { start: 0, end: 0 } }
}

/// `path` with `.` and `..` segments applied; `None` if it leaves the vault.
pub fn normalize_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Where the note is made that `link`, written in the note `source`, names and the vault has
/// not: at the written path for links with a folder (Markdown links: from the linking note's
/// folder), else in the folder of new notes (`inbox`). None for a link that names nothing, or
/// a place outside the vault.
pub fn linked_note_path(link: &Link, source: &str, inbox: &str) -> Option<String> {
    let target = link.target.trim().trim_start_matches('/');
    if target.is_empty() {
        return None;
    }
    let folder = match link.kind {
        LinkKind::Markdown => source.rsplit_once('/').map_or("", |(dir, _)| dir),
        _ if target.contains('/') => "",
        _ => inbox,
    };
    let mut path = normalize_path(&if folder.is_empty() { target.to_owned() } else { format!("{folder}/{target}") })?;
    if !path.to_lowercase().ends_with(".md") {
        path.push_str(".md");
    }
    Some(path)
}
