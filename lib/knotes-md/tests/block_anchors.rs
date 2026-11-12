use knotes_core::NoteParser;
use knotes_md::MarkdownParser;

fn anchors(source: &str) -> Vec<String> {
    MarkdownParser::new().parse(source).anchors.into_iter().map(|a| a.name).collect()
}

#[test]
fn anchor_at_end_of_paragraph() {
    let src = "para text ^abc-1\n";
    let note = MarkdownParser::new().parse(src);
    assert_eq!(note.anchors[0].name, "abc-1");
    let s = note.anchors[0].span;
    assert_eq!(&src[s.start as usize..s.end as usize], "^abc-1");
}

#[test]
fn anchor_at_end_of_list_item_and_at_end_of_file() {
    assert_eq!(anchors("- item ^li1\n- item two ^li2"), ["li1", "li2"]);
}

#[test]
fn anchor_on_its_own_line_after_a_block() {
    assert_eq!(anchors("| a |\n|---|\n| b |\n\n^table1\n"), ["table1"]);
}

#[test]
fn caret_in_the_middle_or_inside_words_is_not_an_anchor() {
    assert!(anchors("x ^notid more\n\na^b\n\n2^10 = 1024\n").is_empty());
}

#[test]
fn no_anchors_in_code() {
    assert!(anchors("`code ^x`\n\n```\nline ^y\n```\n").is_empty());
}
