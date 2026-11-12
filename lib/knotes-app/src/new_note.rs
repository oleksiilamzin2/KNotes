//! Names of new notes (ADR 0011): a file name from the first line, unique in its folder.

/// Longest file name taken from a first line, in characters (without `.md`).
const MAX_CHARS: usize = 100;

/// File name (without `.md`) for a note whose first line is `first_line`: block markers
/// (`#`, `>`, list bullets, task boxes) and inline markup are dropped, as are characters that
/// break file names or Obsidian links (`/` becomes `-`). `None` when nothing is left.
pub fn note_file_stem(first_line: &str) -> Option<String> {
    let mut line = first_line.trim().trim_start_matches('#').trim_start();
    line = line.trim_start_matches('>').trim_start();
    if let Some(rest) = ["- ", "* ", "+ "].iter().find_map(|b| line.strip_prefix(b)) {
        line = rest;
    } else if let Some((number, rest)) = line.split_once(". ")
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
    {
        line = rest;
    }
    if let Some(rest) = ["[ ] ", "[x] ", "[X] "].iter().find_map(|t| line.strip_prefix(t)) {
        line = rest;
    }
    let cleaned: String = line
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            '*' | '_' | '`' | '~' | '[' | ']' | '#' | '^' | '|' | ':' | '?' | '"' | '<' | '>' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut stem = String::new();
    let mut length = 0;
    for word in cleaned.split_whitespace() {
        let words = word.chars().count();
        let next = if stem.is_empty() { words } else { length + 1 + words };
        if next > MAX_CHARS {
            if stem.is_empty() {
                stem = word.chars().take(MAX_CHARS).collect();
            }
            break;
        }
        if !stem.is_empty() {
            stem.push(' ');
        }
        stem.push_str(word);
        length = next;
    }
    // No dot files (the scanner skips them) and no trailing dots.
    let stem = stem.trim_matches(|c: char| c == '.' || c.is_whitespace());
    (!stem.is_empty()).then(|| stem.to_owned())
}

/// `folder/stem.md`, or `folder/stem N.md` with the first free number.
pub fn unique_note_path(folder: &str, stem: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = if folder.is_empty() { stem.to_owned() } else { format!("{folder}/{stem}") };
    let mut path = format!("{base}.md");
    let mut n = 1;
    while exists(&path) {
        path = format!("{base} {n}.md");
        n += 1;
    }
    path
}
