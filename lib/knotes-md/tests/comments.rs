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
