use knotes_editor::{Edit, changed_region, highlight};

/// Region after replacing `removed` UTF-16 units at `at` with `inserted` text.
fn region(before: &str, at: usize, removed: usize, inserted: &str) -> (usize, usize) {
    let after = format!("{}{}{}", &before[..at], inserted, &before[at + removed..]);
    let old = highlight(before).to_utf16(before);
    let new = highlight(&after).to_utf16(&after);
    changed_region(&old, &new, Edit { location: at, removed, inserted: inserted.encode_utf16().count() })
}

#[test]
fn plain_typing_touches_only_the_typed_text() {
    let before = "# Title\n\nsome text\n\n**bold** after";
    assert_eq!(region(before, 13, 0, "x"), (13, 14));
}

#[test]
fn styled_text_after_the_edit_only_moves_and_is_left_alone() {
    let before = "abc\n\n**bold** and [[Link]]";
    assert_eq!(region(before, 1, 0, "xyz"), (1, 4));
}

#[test]
fn closing_a_delimiter_restyles_the_new_run() {
    let before = "say **b* end";
    // "say **b** end": the strong run and its markup start at 4.
    assert_eq!(region(before, 8, 0, "*"), (4, 9));
}

#[test]
fn opening_a_code_fence_restyles_everything_below() {
    let before = "a\n\nb **c**\n\nd";
    let end = before.len() + 4;
    assert_eq!(region(before, 2, 0, "```\n").1, end);
}

#[test]
fn removing_a_setext_underline_restyles_the_line_above() {
    let before = "Title\n===\n\nbody";
    let (start, end) = region(before, 5, 4, "");
    assert_eq!(start, 0, "the heading style must leave `Title`");
    assert!(end >= 5);
}
