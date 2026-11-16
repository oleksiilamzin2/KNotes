use knotes_editor::{TagCompletion, tag_completion, tag_typed};

fn inline(from: usize, query: &str) -> Option<TagCompletion> {
    Some(TagCompletion { from, query: query.to_owned(), frontmatter: false })
}

fn listed(from: usize, query: &str) -> Option<TagCompletion> {
    Some(TagCompletion { from, query: query.to_owned(), frontmatter: true })
}

#[test]
fn a_hash_at_a_word_s_start_completes_the_tag_typed_after_it() {
    assert_eq!(tag_completion("Plan #pro"), inline(6, "pro"));
    assert_eq!(tag_completion("#"), inline(1, ""), "nothing typed yet: every tag");
    assert_eq!(tag_completion("first\nsee #area/su"), inline(11, "area/su"), "a nested tag; offsets in the whole text");
    assert_eq!(tag_completion("#робо"), inline(1, "робо"));
    assert_eq!(tag_completion("a#b"), None, "not at a word's start");
    assert_eq!(tag_completion("## Head"), None, "a heading");
    assert_eq!(tag_completion("#pro ject"), None, "the tag ended");
    assert_eq!(tag_completion("x \\#pro"), None, "escaped");
}

#[test]
fn no_tag_is_completed_in_code() {
    assert_eq!(tag_completion("run `x #pro"), None, "inline code");
    assert_eq!(tag_completion("```\nlet #pro"), None, "a fenced block");
    assert_eq!(tag_completion("```\ncode\n```\nafter #pro"), inline(20, "pro"), "after the block");
}

#[test]
fn in_the_frontmatter_the_tags_values_complete() {
    assert_eq!(tag_completion("---\ntags: [work, pro"), listed(17, "pro"), "a flow list");
    assert_eq!(tag_completion("---\ntags: [\"pro"), listed(12, "pro"), "quoted");
    assert_eq!(tag_completion("---\ntags:\n  - work\n  - pr"), listed(23, "pr"), "a block list");
    assert_eq!(tag_completion("---\ntag: work pr"), listed(14, "pr"), "the old way, words apart");
    assert_eq!(tag_completion("---\ntags: [#pro"), listed(12, "pro"), "a hash before it, as Obsidian takes it");
    assert_eq!(tag_completion("---\ntitle: #x"), None, "another key");
    assert_eq!(tag_completion("---\naliases:\n  - pr"), None, "another key's list");
    assert_eq!(tag_completion("---\ntags: [a]\n---\nbody #t"), inline(24, "t"), "after the frontmatter: the text's");
}

#[test]
fn only_a_line_ending_in_a_tag_needs_the_note_read_up_to_it() {
    assert!(tag_typed("Plan #pro") && tag_typed("#"));
    assert!(!tag_typed("Plan pro") && !tag_typed("a#b"));
}
