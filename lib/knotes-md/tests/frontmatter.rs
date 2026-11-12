use knotes_core::{Frontmatter, NoteParser};
use knotes_md::MarkdownParser;

fn fm(source: &str) -> Frontmatter {
    MarkdownParser::new().parse(source).frontmatter.expect("frontmatter")
}

#[test]
fn no_frontmatter() {
    assert_eq!(MarkdownParser::new().parse("# Title\n").frontmatter, None);
}

#[test]
fn span_covers_the_whole_block() {
    let src = "---\ntags: [a]\n---\n\nbody\n";
    let s = fm(src).span;
    assert_eq!(&src[s.start as usize..s.end as usize], "---\ntags: [a]\n---");
}

#[test]
fn inline_list_tags_strip_hash() {
    assert_eq!(fm("---\ntags: [a, \"#b\", c/d]\n---\n").tags, ["a", "b", "c/d"]);
}

#[test]
fn block_list_tags() {
    assert_eq!(fm("---\ntags:\n  - one\n  - проєкт/радар\n---\n").tags, ["one", "проєкт/радар"]);
}

#[test]
fn string_tags_split_on_commas_and_spaces() {
    assert_eq!(fm("---\ntags: one, two three\n---\n").tags, ["one", "two", "three"]);
}

#[test]
fn legacy_singular_keys() {
    let f = fm("---\ntag: x\nalias: Other name\n---\n");
    assert_eq!(f.tags, ["x"]);
    assert_eq!(f.aliases, ["Other name"]);
}

#[test]
fn aliases_list_keeps_spaces() {
    assert_eq!(fm("---\naliases:\n  - First name\n  - Друга назва\n---\n").aliases, ["First name", "Друга назва"]);
}

#[test]
fn aliases_string_splits_on_commas_only() {
    assert_eq!(fm("---\naliases: First name, Second\n---\n").aliases, ["First name", "Second"]);
}

#[test]
fn numeric_only_tags_are_dropped() {
    assert_eq!(fm("---\ntags: [2024, y2024]\n---\n").tags, ["y2024"]);
}

#[test]
fn invalid_yaml_keeps_frontmatter_without_values() {
    let f = fm("---\ntags: [unclosed\n---\n");
    assert!(f.tags.is_empty() && f.aliases.is_empty());
}
