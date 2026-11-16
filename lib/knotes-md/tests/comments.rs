use knotes_core::NoteParser;
use knotes_md::{MarkdownParser, comment_ranges};

#[test]
fn comments_run_between_double_percents_and_an_open_one_to_the_end() {
    let text = |src: &'static str, verbatim: &[std::ops::Range<usize>]| comment_ranges(src, verbatim).into_iter().map(|r| &src[r]).collect::<Vec<_>>();
    assert_eq!(text("a %%one%% b %%two%%", &[]), ["%%one%%", "%%two%%"]);
    assert_eq!(text("a %%\nblock\n%% b", &[]), ["%%\nblock\n%%"], "across lines");
    assert_eq!(text("a %%open to the end", &[]), ["%%open to the end"], "as Obsidian: an open comment runs to the end");
    let src = "`%%` x %%c%%";
    let code = 0..4;
    assert_eq!(text(src, std::slice::from_ref(&code)), ["%%c%%"], "not in code");
}

#[test]
fn tags_links_anchors_and_headings_in_comments_are_not_the_note_s() {
    let note = MarkdownParser::new().parse("#real %% #hidden [[Secret]] ^gone %%\n\n%%\n# Hidden\n[[Also]]\n%%\n\n`%%` #after [[Kept]]\n");
    assert_eq!(note.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["real", "after"]);
    assert_eq!(note.links.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(), ["Kept"]);
    assert!(note.anchors.is_empty() && note.headings.is_empty(), "{:?} {:?}", note.anchors, note.headings);
}

#[test]
fn highlights_are_double_equals_around_words_within_a_line() {
    use knotes_md::mark_ranges;
    let text = |src: &'static str| mark_ranges(src, &[]).into_iter().map(|r| &src[r]).collect::<Vec<_>>();
    assert_eq!(text("a ==one== b ==two words=="), ["==one==", "==two words=="]);
    assert!(text("a == b == c").is_empty(), "signs before and after a space");
    assert!(text("Title\n====\n").is_empty(), "a heading's underline");
    assert!(text("a ==open\nshut== b").is_empty(), "within a line");
}

#[test]
fn a_highlight_sign_inside_a_comment_holding_code_is_skipped() {
    use knotes_md::mark_ranges;
    let src = "%% `x` ==no== %%";
    let comment = 0..src.len();
    let code = 3..6;
    assert!(mark_ranges(src, &[comment, code]).is_empty());
}
