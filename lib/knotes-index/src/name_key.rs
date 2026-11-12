//! Keys for matching link targets with file names: NFC (macOS may hand out decomposed names,
//! e.g. `й` as `и` + combining breve) and lowercase (Obsidian links ignore case).

use unicode_normalization::UnicodeNormalization;

pub(crate) fn fold(s: &str) -> String {
    s.nfc().flat_map(char::to_lowercase).collect()
}

/// File name used for linking: the last path segment, without `.md` for notes.
pub(crate) fn file_name_key(path: &str) -> String {
    fold(strip_md(path.rsplit('/').next().unwrap_or(path)))
}

/// Key of a link target, `None` for links inside the same note (`[[#Heading]]`).
pub(crate) fn link_target_key(target: &str) -> Option<String> {
    let name = target.rsplit('/').next().unwrap_or(target);
    (!name.is_empty()).then(|| fold(strip_md(name)))
}

pub(crate) fn strip_md(name: &str) -> &str {
    match name.len().checked_sub(3) {
        Some(cut) if name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(".md") => &name[..cut],
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(file_name_key("Folder/My Note.MD"), "my note");
        assert_eq!(file_name_key("att/Diagram.PNG"), "diagram.png");
        assert_eq!(link_target_key("Folder/Note.md").as_deref(), Some("note"));
        assert_eq!(link_target_key(""), None);
        assert_eq!(file_name_key("И\u{0306}од.md"), "йод");
    }
}
