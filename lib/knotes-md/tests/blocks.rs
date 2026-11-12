use knotes_core::NoteParser;
use knotes_md::MarkdownParser;

/// (block text trimmed, index of its section heading)
fn blocks(source: &str, max: usize) -> Vec<(String, Option<u32>)> {
    MarkdownParser::with_max_block_bytes(max)
        .parse(source)
        .blocks
        .into_iter()
        .map(|b| (source[b.span.start as usize..b.span.end as usize].trim().to_owned(), b.heading))
        .collect()
}

fn texts(source: &str, max: usize) -> Vec<String> {
    blocks(source, max).into_iter().map(|b| b.0).collect()
}

#[test]
fn one_block_per_heading_section() {
    let src = "pre\n\n# A\n\ntext a\n\n## B\n\ntext b\n";
    assert_eq!(
        blocks(src, 2048),
        [
            ("pre".to_owned(), None),
            ("# A\n\ntext a".to_owned(), Some(0)),
            ("## B\n\ntext b".to_owned(), Some(1)),
        ]
    );
}

#[test]
fn frontmatter_is_not_a_block() {
    assert_eq!(texts("---\nt: 1\n---\n\nbody\n", 2048), ["body"]);
}

#[test]
fn empty_note_has_no_blocks() {
    assert!(texts("", 2048).is_empty());
    assert!(texts("\n\n", 2048).is_empty());
}

#[test]
fn hash_in_code_block_stays_in_section() {
    let src = "# A\n\n```\n# not heading\n```\n";
    assert_eq!(blocks(src, 2048), [("# A\n\n```\n# not heading\n```".to_owned(), Some(0))]);
}

#[test]
fn long_section_splits_at_block_boundaries_and_keeps_heading() {
    let src = "# H\n\npara one here\n\npara two here\n\npara three\n";
    assert_eq!(
        blocks(src, 24),
        [
            ("# H\n\npara one here".to_owned(), Some(0)),
            ("para two here".to_owned(), Some(0)),
            ("para three".to_owned(), Some(0)),
        ]
    );
}

#[test]
fn oversized_paragraph_splits_at_whitespace_within_limit() {
    let src = "aaaa bbbb cccc dddd eeee\n";
    let note = MarkdownParser::with_max_block_bytes(10).parse(src);
    assert!(note.blocks.len() > 1);
    for b in &note.blocks {
        assert!(b.span.end - b.span.start <= 10, "{b:?}");
    }
    let joined: Vec<&str> = note
        .blocks
        .iter()
        .flat_map(|b| src[b.span.start as usize..b.span.end as usize].split_whitespace())
        .collect();
    assert_eq!(joined, ["aaaa", "bbbb", "cccc", "dddd", "eeee"]);
}

#[test]
fn oversized_text_without_spaces_splits_on_char_boundaries() {
    let src = "жжжжжжжжжж\n";
    let note = MarkdownParser::with_max_block_bytes(5).parse(src);
    let pieces: String = note.blocks.iter().map(|b| &src[b.span.start as usize..b.span.end as usize]).collect();
    assert_eq!(pieces.trim_end(), "жжжжжжжжжж");
    assert!(note.blocks.iter().all(|b| b.span.end - b.span.start <= 5));
}
