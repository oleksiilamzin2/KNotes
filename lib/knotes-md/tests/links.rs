use knotes_core::{Link, LinkKind, NoteParser, Span};
use knotes_md::MarkdownParser;

fn links(source: &str) -> Vec<Link> {
    MarkdownParser::new().parse(source).links
}

fn one(source: &str) -> Link {
    let mut l = links(source);
    assert_eq!(l.len(), 1, "expected one link in {source:?}, got {l:?}");
    l.remove(0)
}

fn src_of(source: &str, span: Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

#[test]
fn plain_wikilink() {
    let src = "see [[Note]] here";
    let l = one(src);
    assert_eq!((l.kind, l.embed), (LinkKind::Wiki, false));
    assert_eq!(l.target, "Note");
    assert_eq!(l.subpath, None);
    assert_eq!(l.display, None);
    assert_eq!(src_of(src, l.span), "[[Note]]");
}

#[test]
fn wikilink_with_path_heading_and_alias() {
    let l = one("[[Folder/Note#Heading two|alias]]");
    assert_eq!(l.target, "Folder/Note");
    assert_eq!(l.subpath.as_deref(), Some("Heading two"));
    assert_eq!(l.display.as_deref(), Some("alias"));
}

#[test]
fn wikilink_to_block() {
    assert_eq!(one("[[Note#^abc123]]").subpath.as_deref(), Some("^abc123"));
}

#[test]
fn wikilink_to_heading_in_same_note() {
    let l = one("[[#Local heading]]");
    assert_eq!(l.target, "");
    assert_eq!(l.subpath.as_deref(), Some("Local heading"));
}

#[test]
fn wikilink_keeps_extension_as_written() {
    assert_eq!(one("[[Note.md]]").target, "Note.md");
}

#[test]
fn wiki_embed_with_size() {
    let src = "![[diagram.png|300]]";
    let l = one(src);
    assert_eq!((l.kind, l.embed), (LinkKind::Wiki, true));
    assert_eq!(l.target, "diagram.png");
    assert_eq!(l.display.as_deref(), Some("300"));
    assert_eq!(src_of(src, l.span), src);
}

#[test]
fn markdown_internal_link_is_percent_decoded() {
    let l = one("[text](Folder/My%20Note.md#Sec)");
    assert_eq!((l.kind, l.embed), (LinkKind::Markdown, false));
    assert_eq!(l.target, "Folder/My Note.md");
    assert_eq!(l.subpath.as_deref(), Some("Sec"));
    assert_eq!(l.display.as_deref(), Some("text"));
}

#[test]
fn external_link_keeps_full_url() {
    let l = one("[site](https://example.com/a?b=1#frag)");
    assert_eq!(l.kind, LinkKind::External);
    assert_eq!(l.target, "https://example.com/a?b=1#frag");
    assert_eq!(l.subpath, None);
    assert_eq!(l.display.as_deref(), Some("site"));
}

#[test]
fn autolink_is_external_without_display() {
    let l = one("<https://example.com>");
    assert_eq!(l.kind, LinkKind::External);
    assert_eq!(l.display, None);
}

#[test]
fn markdown_image_is_an_embed() {
    let l = one("![alt text](img/pic.jpg)");
    assert_eq!((l.kind, l.embed), (LinkKind::Markdown, true));
    assert_eq!(l.target, "img/pic.jpg");
    assert_eq!(l.display.as_deref(), Some("alt text"));
}

#[test]
fn links_in_code_are_ignored() {
    assert!(links("`[[Inline]]`\n\n```\n[[Block]] [a](b.md)\n```\n").is_empty());
}

#[test]
fn ukrainian_wikilink() {
    let l = one("[[Нотатка про FMCW|радар]]");
    assert_eq!(l.target, "Нотатка про FMCW");
    assert_eq!(l.display.as_deref(), Some("радар"));
}

#[test]
fn link_inside_heading_is_collected() {
    let note = MarkdownParser::new().parse("## About [[Note|alias]]\n");
    assert_eq!(note.headings[0].text, "About alias");
    assert_eq!(note.links[0].target, "Note");
}

#[test]
fn wikilink_with_escaped_pipe_in_table() {
    let src = "| a | b |\n|---|---|\n| [[Note\\|alias]] | x |\n";
    let l = one(src);
    assert_eq!(l.target, "Note");
    assert_eq!(l.display.as_deref(), Some("alias"));
}

#[test]
fn image_inside_link_gives_two_links() {
    let l = links("[![logo](logo.png)](Home.md)");
    let mut targets: Vec<_> = l.iter().map(|l| (l.target.as_str(), l.embed)).collect();
    targets.sort();
    assert_eq!(targets, [("Home.md", false), ("logo.png", true)]);
}

fn external(source: &str) -> Vec<String> {
    links(source).into_iter().filter(|l| l.kind == LinkKind::External).map(|l| l.target).collect()
}

#[test]
fn bare_urls_in_prose_are_links_to_websites() {
    assert_eq!(external("see https://example.com/a?b=1, and http://x.y."), ["https://example.com/a?b=1", "http://x.y"]);
    assert_eq!(external("(see https://en.wikipedia.org/wiki/Rust_(language)) and more"), ["https://en.wikipedia.org/wiki/Rust_(language)"]);
    let src = "go to https://example.com now";
    let l = one(src);
    assert_eq!((l.kind, src_of(src, l.span), l.display.as_deref()), (LinkKind::External, "https://example.com", None));
}

#[test]
fn urls_in_code_or_in_links_are_not_bare_links() {
    assert!(external("`https://in.code` and\n\n```\nhttps://in.block\n```").is_empty());
    assert_eq!(external("[text](https://x.y) and <https://a.b>"), ["https://x.y", "https://a.b"], "each link once");
}
