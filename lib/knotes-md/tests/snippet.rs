use knotes_md::snippet;

#[test]
fn skips_frontmatter_title_and_markup() {
    let src = "---\ntags: x\n---\n# Title\n\nFirst **bold** line with [[Link|alias]] and [[Plain]].\nSecond line\n";
    assert_eq!(snippet(src, 200), "First bold line with alias and Plain. Second line");
}

#[test]
fn headings_and_code_are_skipped_lists_are_text() {
    assert_eq!(snippet("## Sub\n\n```\ncode\n```\n\n- item one\n- item two\n", 200), "item one item two");
}

#[test]
fn long_text_is_cut_on_a_character_boundary() {
    let s = snippet(&"жжжж ".repeat(100), 10);
    assert_eq!(s.chars().count(), 10);
    assert!(s.starts_with("жжжж"));
}

#[test]
fn empty_and_heading_only_notes() {
    assert_eq!(snippet("", 200), "");
    assert_eq!(snippet("# Only a title\n", 200), "");
}

#[test]
fn parser_fills_the_snippet() {
    use knotes_core::NoteParser;
    let src = "# T\n\nbody text\n";
    assert_eq!(knotes_md::MarkdownParser::new().parse(src).snippet, "body text");
}

#[test]
fn the_preview_leaves_out_comments_and_the_signs_of_highlights() {
    assert_eq!(knotes_md::snippet("Це ==важлива== думка %%таємне #тег%% далі, a == b", 200), "Це важлива думка далі, a == b");
}

#[test]
fn the_preview_leaves_out_what_is_embedded() {
    assert_eq!(knotes_md::snippet("Top\n\n![[b#Part]]\n\n![alt text](pic.png)\n\nend", 200), "Top end");
}
