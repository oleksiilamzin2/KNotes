use knotes_editor::subpath_offset;

const NOTE: &str = "# Title\n\nintro\n\n## Друга частина\n\ntext line ^b-1\n\n### Last\n";

#[test]
fn headings_are_found_by_text_ignoring_case() {
    assert_eq!(subpath_offset(NOTE, "Title"), Some(0));
    assert_eq!(subpath_offset(NOTE, "друга ЧАСТИНА"), Some(NOTE.find("## Друга").unwrap()));
    assert_eq!(subpath_offset(NOTE, "Last"), Some(NOTE.find("### Last").unwrap()));
}

#[test]
fn block_ids_lead_to_the_start_of_their_line() {
    assert_eq!(subpath_offset(NOTE, "^b-1"), Some(NOTE.find("text line").unwrap()));
}

#[test]
fn unknown_subpaths_find_nothing() {
    assert_eq!(subpath_offset(NOTE, "Nope"), None);
    assert_eq!(subpath_offset(NOTE, "^nope"), None);
}
