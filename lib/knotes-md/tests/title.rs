use knotes_core::NoteParser;
use knotes_md::{MarkdownParser, plain_title, title_range};

fn title(src: &str) -> Option<&str> {
    title_range(src).map(|r| &src[r])
}

#[test]
fn the_title_is_the_first_line_of_a_first_heading_or_paragraph() {
    assert_eq!(title("# My note\n\ntext"), Some("# My note"));
    assert_eq!(title("First line\nstill the paragraph"), Some("First line"));
    assert_eq!(title("---\na: 1\n---\n\n## Below frontmatter\n"), Some("## Below frontmatter"));
    assert_eq!(title("- item\n"), None);
    assert_eq!(title("```\ncode\n```"), None);
}

#[test]
fn a_line_of_tags_and_links_alone_names_nothing() {
    assert_eq!(title("#daily #work\n\ntext"), None);
    assert_eq!(title("[[Project]] #work\n"), None);
    assert_eq!(title("Meeting #work\n"), Some("Meeting #work"));
}

#[test]
fn tags_and_links_in_the_title_are_not_the_notes() {
    let note = MarkdownParser::new().parse("Meeting #work about [[Project|proj]]\n\nbody #real and [[Other]]");
    assert_eq!(note.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["real"]);
    assert_eq!(note.links.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(), ["Other"]);
    let tag_line = MarkdownParser::new().parse("#daily #work\n\nbody");
    assert_eq!(tag_line.tags.len(), 2, "a tag line is no title: its tags count");
}

#[test]
fn a_titles_plain_words_drop_tags_and_keep_what_links_show() {
    assert_eq!(plain_title("Meeting #work about [[Project|proj]] and [[Bob]] **now** ![[img.png]]"), "Meeting about proj and Bob now");
    assert_eq!(plain_title("## Heading with [text](https://x.y)"), "Heading with text");
}
