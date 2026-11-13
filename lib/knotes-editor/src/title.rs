//! The note's title, its first line (ADR 0019), renamed with its file.

use crate::highlight;

/// `text` with its title's words (after any `#` markers) replaced by `name`, when `was_name`
/// says they were the note's old name; `None` when the note has no title or another one.
pub fn replace_title(text: &str, name: &str, was_name: impl Fn(&str) -> bool) -> Option<String> {
    let title = highlight(text).title?;
    let line = &text[title.clone()];
    let words = line.trim_start_matches('#').trim_start();
    let start = title.end - words.len();
    was_name(words).then(|| format!("{}{name}{}", &text[..start], &text[title.end..]))
}
