//! Helpers that turn a raw link destination into `(target, subpath)`.

use std::borrow::Cow;

/// RFC 3986 scheme followed by `:`. At least two characters, so `C:\` is not a scheme.
pub(crate) fn has_scheme(dest: &str) -> bool {
    let Some(colon) = dest.find(':') else { return false };
    let scheme = &dest.as_bytes()[..colon];
    scheme.len() >= 2
        && scheme[0].is_ascii_alphabetic()
        && scheme.iter().all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// Splits `path#subpath` at the first `#`. Empty subpath counts as none.
pub(crate) fn split_subpath(dest: &str) -> (&str, Option<&str>) {
    match dest.split_once('#') {
        Some((path, sub)) if !sub.is_empty() => (path, Some(sub)),
        Some((path, _)) => (path, None),
        None => (dest, None),
    }
}

/// Decodes `%XX` escapes; invalid escapes are kept as written.
pub(crate) fn percent_decode(s: &str) -> Cow<'_, str> {
    if !s.contains('%') {
        return Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|&b| hex(b)),
                bytes.get(i + 2).and_then(|&b| hex(b)),
            )
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Cow::Owned(String::from_utf8_lossy(&out).into_owned())
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_detection() {
        assert!(has_scheme("https://x"));
        assert!(has_scheme("mailto:a@b"));
        assert!(has_scheme("obsidian://open"));
        assert!(!has_scheme("C:\\notes"));
        assert!(!has_scheme("Folder/Note.md"));
        assert!(!has_scheme("Note: draft.md".split_once(' ').unwrap().1));
    }

    #[test]
    fn percent_decoding_handles_utf8_and_bad_escapes() {
        assert_eq!(percent_decode("My%20Note"), "My Note");
        assert_eq!(percent_decode("%D0%A0%D0%B0"), "Ра");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
