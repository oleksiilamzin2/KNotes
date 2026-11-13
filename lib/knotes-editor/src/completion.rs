//! What to complete while typing a wikilink (ADR 0013).

/// Completion at the cursor; `from` is the byte offset in the text before the cursor where
/// the replaced part starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkCompletion {
    /// `[[query`: a note title.
    Title { from: usize, query: String },
    /// `[[target#query`: a heading of `target` (`""`: the note itself).
    Heading { from: usize, target: String, query: String },
    /// `[[target#^query`: a block anchor of `target`; `from` is after the `#`.
    Block { from: usize, target: String, query: String },
}

/// `before`: the cursor's line up to the cursor. Completes inside the last `[[` that is not
/// closed yet, unless an alias (`|`) is being typed.
pub fn link_completion(before: &str) -> Option<LinkCompletion> {
    let open = before.rfind("[[")? + 2;
    let inside = &before[open..];
    if inside.contains("]]") || inside.contains('|') {
        return None;
    }
    match inside.split_once('#') {
        None => Some(LinkCompletion::Title { from: open, query: inside.to_owned() }),
        Some((target, block)) if block.starts_with('^') => {
            Some(LinkCompletion::Block { from: open + target.len() + 1, target: target.to_owned(), query: block[1..].to_owned() })
        }
        Some((target, heading)) => {
            Some(LinkCompletion::Heading { from: open + target.len() + 1, target: target.to_owned(), query: heading.to_owned() })
        }
    }
}
