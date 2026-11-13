//! Links to a renamed or moved note, rewritten to its new path in the form each had (ADR 0019).

use knotes_core::{Link, LinkKind, LinkResolver};

/// `text` of the note `source` with each of `links` (links to the note at `from`) pointing to
/// `to` instead. `after` resolves links in the vault as it is after the move. A wikilink's
/// bare name stays bare while it is unique, a full path stays full, `.md`, `#…` and `|…`
/// stay; a Markdown link keeps its relative or root form and its brackets.
pub fn rewrite_links(text: &str, source: &str, links: &[Link], from: &str, to: &str, after: &dyn LinkResolver) -> String {
    let mut out = text.to_owned();
    let mut links: Vec<&Link> = links.iter().collect();
    // From the end, so earlier spans stay where they were.
    links.sort_by_key(|l| std::cmp::Reverse(l.span.start));
    for link in links {
        let span = link.span.start as usize..link.span.end as usize;
        let Some(raw) = text.get(span.clone()) else { continue };
        let new = match link.kind {
            LinkKind::Wiki => wikilink(raw, source, from, to, after),
            LinkKind::Markdown => markdown_link(raw, source, from, to),
            LinkKind::External => None,
        };
        if let Some(new) = new {
            out.replace_range(span, &new);
        }
    }
    out
}

/// `[[target#…|…]]` or `![[…]]` with the target rewritten.
fn wikilink(raw: &str, source: &str, from: &str, to: &str, after: &dyn LinkResolver) -> Option<String> {
    let open = if raw.starts_with("![[") { 3 } else if raw.starts_with("[[") { 2 } else { return None };
    let inner = raw.get(open..raw.len().checked_sub(2)?)?;
    // The target ends at a heading, a block or an alias (`\|` in a table cell).
    let end = inner.find(['#', '|']).map_or(inner.len(), |i| if i > 0 && inner.as_bytes()[i - 1] == b'\\' { i - 1 } else { i });
    let written = &inner[..end];
    if written.trim().is_empty() {
        return None;
    }
    let md = if written.to_lowercase().ends_with(".md") { ".md" } else { "" };
    let full = written.contains('/') && strip_md(written.trim_start_matches('/')).eq_ignore_ascii_case(strip_md(from));
    let target = if full {
        let slash = if written.starts_with('/') { "/" } else { "" };
        format!("{slash}{}", strip_md(to))
    } else {
        after.link_text(to, source)
    };
    Some(format!("{}{target}{md}{}", &raw[..open], &raw[open + end..]))
}

/// `[text](dest#…)` or `![…](<dest> "title")` with the destination's path rewritten.
fn markdown_link(raw: &str, source: &str, from: &str, to: &str) -> Option<String> {
    let open = raw.rfind("](")? + 2;
    let area = raw.get(open..raw.len().checked_sub(1)?)?;
    let angle = area.starts_with('<');
    let dest_end = if angle { area.find('>')? + 1 } else { area.find(char::is_whitespace).unwrap_or(area.len()) };
    let dest = if angle { &area[1..dest_end - 1] } else { &area[..dest_end] };
    let (path, fragment) = dest.split_once('#').map_or((dest, ""), |(p, f)| (p, f));
    let decoded = percent_decode(path);
    let folder = source.rsplit_once('/').map_or("", |(dir, _)| dir);
    let relative = join(folder, &decoded).is_some_and(|p| p.eq_ignore_ascii_case(from));
    let new_path = if relative {
        relative_path(folder, to)
    } else {
        let slash = if decoded.starts_with('/') { "/" } else { "" };
        format!("{slash}{to}")
    };
    let new_path = if angle { new_path } else { new_path.replace(' ', "%20") };
    let fragment = if dest.contains('#') { format!("#{fragment}") } else { String::new() };
    let new_dest = if angle { format!("<{new_path}{fragment}>") } else { format!("{new_path}{fragment}") };
    Some(format!("{}{new_dest}{})", &raw[..open], &area[dest_end..]))
}

fn strip_md(path: &str) -> &str {
    path.strip_suffix(".md").or_else(|| path.strip_suffix(".MD")).unwrap_or(path)
}

/// `relative` resolved against `folder` (`.` and `..` taken out); `None` above the vault.
fn join(folder: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The path from `folder` to the vault path `to`.
fn relative_path(folder: &str, to: &str) -> String {
    let from: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    let target: Vec<&str> = to.split('/').collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts = vec![".."; from.len() - common];
    parts.extend(&target[common..]);
    parts.join("/")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (bytes.get(i + 1).and_then(|&b| hex(b)), bytes.get(i + 2).and_then(|&b| hex(b)))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
