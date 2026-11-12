use knotes_core::{Heading, NoteParser, Span};
use knotes_md::MarkdownParser;

fn headings(source: &str) -> Vec<Heading> {
    MarkdownParser::new().parse(source).headings
}

#[test]
fn extracts_atx_headings_with_level_text_and_span() {
    let src = "# Title\n\ntext\n\n## Sub section\n";
    assert_eq!(
        headings(src),
        vec![
            Heading { level: 1, text: "Title".into(), span: Span::new(0, 8) },
            Heading { level: 2, text: "Sub section".into(), span: Span::new(15, 30) },
        ]
    );
}

#[test]
fn extracts_setext_headings() {
    let h = headings("Big\n===\n\nSmall\n---\n");
    assert_eq!(h.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(), [(1, "Big"), (2, "Small")]);
}

#[test]
fn heading_text_drops_inline_markup_but_keeps_code_and_link_text() {
    let h = headings("## Hello *big* `code` [[Note|alias]] [site](https://x.y) $x^2$\n");
    assert_eq!(h[0].text, "Hello big code alias site x^2");
}

#[test]
fn hashes_inside_code_blocks_are_not_headings() {
    let src = "```\n# not a heading\n```\n\n    # indented code\n";
    assert!(headings(src).is_empty());
}

#[test]
fn ukrainian_heading_text_and_byte_span() {
    let src = "текст\n\n### Розділ два\n";
    let h = headings(src);
    assert_eq!(h[0].text, "Розділ два");
    assert_eq!(&src[h[0].span.start as usize..h[0].span.end as usize], "### Розділ два\n");
}

#[test]
fn frontmatter_is_not_a_setext_heading() {
    let src = "---\ntags: [a]\n---\n\n# Real\n";
    assert_eq!(headings(src).iter().map(|h| h.text.as_str()).collect::<Vec<_>>(), ["Real"]);
}
