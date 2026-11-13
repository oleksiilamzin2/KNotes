use knotes_core::NoteParser;
use knotes_md::MarkdownParser;

/// Tags of `source` written below a title line (the title's tags are its name's, ADR 0019).
fn tags(source: &str) -> Vec<String> {
    MarkdownParser::new().parse(&format!("Title\n\n{source}")).tags.into_iter().map(|t| t.name).collect()
}

#[test]
fn plain_and_nested_tags() {
    assert_eq!(tags("text #tag and #nested/sub-tag_1 end"), ["tag", "nested/sub-tag_1"]);
}

#[test]
fn ukrainian_tags() {
    assert_eq!(tags("план #проєкт/радар готовий"), ["проєкт/радар"]);
}

#[test]
fn tag_span_points_at_hash() {
    let src = "Title\n\na #tag, b";
    let note = MarkdownParser::new().parse(src);
    let s = note.tags[0].span;
    assert_eq!(&src[s.start as usize..s.end as usize], "#tag");
}

#[test]
fn numbers_words_and_urls_are_not_tags() {
    assert!(tags("#123 C#sharp issue#5 https://x.y/#frag ##double").is_empty());
}

#[test]
fn tag_may_start_with_digit_if_not_only_digits() {
    assert_eq!(tags("#1a"), ["1a"]);
}

#[test]
fn punctuation_and_trailing_slash_end_a_tag() {
    assert_eq!(tags("#one, #two. #three/"), ["one", "two", "three"]);
}

#[test]
fn underscore_does_not_split_a_tag() {
    assert_eq!(tags("see #my_tag_here now"), ["my_tag_here"]);
}

#[test]
fn tag_at_start_of_wrapped_line() {
    assert_eq!(tags("line one\n#todo next"), ["todo"]);
}

#[test]
fn tags_in_headings_count() {
    assert_eq!(tags("## Plan #todo\n"), ["todo"]);
}

#[test]
fn no_tags_in_code_frontmatter_links_or_escapes() {
    let src = "---\ntitle: '#nofm'\n---\n\n`#nocode`\n\n```\n#noblock\n```\n\n[#nolink](https://x.y) \\#noescape\n";
    assert!(tags(src).is_empty(), "{:?}", tags(src));
}
