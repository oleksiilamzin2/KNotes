//! A card's text as the review shows it: plain text and links, `[[note]]`, `[[note|shown]]` and
//! `[shown](address)`.

/// Where a link in a card's text leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardLink {
    /// A note or a file of the vault, as a wiki link names it (`note`, `note#heading`).
    Note(String),
    /// A web address (`https://…`, `mailto:…`).
    Web(String),
}

/// A piece of a card's text: what shows, and where it leads if it is a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub link: Option<CardLink>,
}

/// `text` in pieces: its links apart from the text around them, each showing as it would in the
/// note (a wiki link its alias or its target, a Markdown link its text).
pub fn text_runs(text: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let found = wiki(rest).or_else(|| markdown(rest));
        match found {
            Some((len, shown, link)) => {
                // An embed's `!` goes with its link.
                if plain.ends_with('!') {
                    plain.pop();
                }
                if !plain.is_empty() {
                    runs.push(Run { text: std::mem::take(&mut plain), link: None });
                }
                runs.push(Run { text: shown, link: Some(link) });
                rest = &rest[len..];
            }
            None => {
                let c = rest.chars().next().expect("not empty");
                plain.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        runs.push(Run { text: plain, link: None });
    }
    runs
}

/// A wiki link `rest` starts with: its length, what it shows, where it leads.
fn wiki(rest: &str) -> Option<(usize, String, CardLink)> {
    let inner = rest.strip_prefix("[[")?;
    let end = inner.find("]]")?;
    let inner = &inner[..end];
    if inner.is_empty() || inner.contains('\n') || inner.contains("[[") {
        return None;
    }
    let (target, shown) = inner.split_once('|').unwrap_or((inner, inner));
    let (target, shown) = (target.trim(), shown.trim());
    (!target.is_empty()).then(|| (end + 4, if shown.is_empty() { target } else { shown }.to_owned(), CardLink::Note(target.to_owned())))
}

/// A Markdown link `rest` starts with: `[shown](address)`.
fn markdown(rest: &str) -> Option<(usize, String, CardLink)> {
    let inner = rest.strip_prefix('[')?;
    let close = inner.find("](")?;
    let shown = &inner[..close];
    let address = &inner[close + 2..];
    let end = address.find(')')?;
    let address = address[..end].trim();
    if shown.is_empty() || shown.contains('\n') || shown.contains('[') || address.is_empty() || address.contains(char::is_whitespace) {
        return None;
    }
    let web = address.split_once(':').is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.bytes().all(|b| b.is_ascii_alphabetic()));
    let link = if web { CardLink::Web(address.to_owned()) } else { CardLink::Note(address.trim_end_matches(".md").replace("%20", " ")) };
    Some((1 + close + 2 + end + 1, shown.to_owned(), link))
}
