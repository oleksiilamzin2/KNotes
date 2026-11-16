//! What to complete while typing a tag (ADR 0033): `#tag` at a word's start in the text, or a
//! value of the frontmatter's `tags` (a flow list, a block list, or the old words apart).

/// A tag being typed: `from` is the byte offset in the text before the cursor where its name
/// starts (after the `#` in the text), `query` the name so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCompletion {
    pub from: usize,
    pub query: String,
    /// In the frontmatter's `tags`, where a tag goes without `#`.
    pub frontmatter: bool,
}

/// Whether `before` (the cursor's line up to it) ends in a tag being typed in the text: the
/// note need be read from its start only then (or in its frontmatter).
pub fn tag_typed(before: &str) -> bool {
    in_text(before).is_some()
}

/// The tag being typed at the end of `head` (the note from its start to the cursor), if any.
pub fn tag_completion(head: &str) -> Option<TagCompletion> {
    if let Some(in_frontmatter) = frontmatter(head) {
        return in_frontmatter;
    }
    let line = head.rfind('\n').map_or(0, |i| i + 1);
    let before = &head[line..];
    let at = in_text(before)?;
    // Not in inline code (an odd number of backticks before it) nor in a fenced block.
    let fences = head[..line].lines().filter(|l| l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~")).count();
    if before[..at].matches('`').count() % 2 == 1 || fences % 2 == 1 {
        return None;
    }
    Some(TagCompletion { from: line + at, query: before[at..].to_owned(), frontmatter: false })
}

/// Where the tag's name starts in `before`: after a `#` at the line's start or after a space,
/// followed by a tag's characters only.
fn in_text(before: &str) -> Option<usize> {
    let name: usize = before.chars().rev().take_while(|&c| is_tag_char(c)).map(char::len_utf8).sum();
    let at = before.len() - name;
    let ahead = before[..at].strip_suffix('#')?;
    ahead.chars().next_back().is_none_or(char::is_whitespace).then_some(at)
}

/// `Some` when the cursor is in the frontmatter: the `tags` value being typed there, if any.
fn frontmatter(head: &str) -> Option<Option<TagCompletion>> {
    let body = head.strip_prefix("---\n").or_else(|| head.strip_prefix("---\r\n"))?;
    // The lines above the cursor's, and its own up to the cursor.
    let (done, line) = match body.rsplit_once('\n') {
        Some((done, line)) => (done.lines().map(str::trim_end).collect::<Vec<_>>(), line),
        None => (Vec::new(), body),
    };
    if done.iter().any(|l| *l == "---" || *l == "...") {
        return None;
    }
    let start = head.len() - line.len();
    // `tags: [a, b` or `tags: a b`: after the last `[`, `,` or (the old way) space.
    let value_at = if let Some(value) = tags_key(line) {
        let value_start = line.len() - value.len();
        let flow = value.trim_start().starts_with('[');
        let cut = value.rfind(|c: char| c == '[' || c == ',' || (!flow && c.is_whitespace())).map_or(0, |i| i + 1);
        value_start + cut
    } else if let Some(item) = line.trim_start().strip_prefix('-') {
        // `  - b` in the list below `tags:` (its items above it).
        let key = done.iter().rev().find(|l| !l.trim_start().starts_with('-'));
        if !key.is_some_and(|k| tags_key(k).is_some_and(|v| v.trim().is_empty())) {
            return Some(None);
        }
        line.len() - item.len()
    } else {
        return Some(None);
    };
    let typed = &line[value_at..];
    let skipped = typed.len() - typed.trim_start_matches([' ', '"', '\'', '#']).len();
    let query = &typed[skipped..];
    if !query.chars().all(is_tag_char) {
        return Some(None);
    }
    Some(Some(TagCompletion { from: start + value_at + skipped, query: query.to_owned(), frontmatter: true }))
}

/// The value after a `tags:` (or `tag:`) key on `line`.
fn tags_key(line: &str) -> Option<&str> {
    line.strip_prefix("tags:").or_else(|| line.strip_prefix("tag:"))
}

/// As the index takes a tag's characters (knotes-md's tag scan).
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}
