use knotes_app::{SearchMode, parse_search, with_search_mode};

#[test]
fn file_names_are_searched_unless_the_query_starts_with_content() {
    assert_eq!(parse_search("радар"), (SearchMode::Files, "радар"));
    assert_eq!(parse_search("file:radar plan"), (SearchMode::Files, "radar plan"));
    assert_eq!(parse_search("files: radar"), (SearchMode::Files, "radar"));
    assert_eq!(parse_search("content: мова"), (SearchMode::Content, "мова"));
    assert_eq!(parse_search("  Content:мова  "), (SearchMode::Content, "мова"), "case and spaces do not matter");
    assert_eq!(parse_search("content:"), (SearchMode::Content, ""));
    assert_eq!(parse_search("contents of"), (SearchMode::Files, "contents of"), "only the operator with a colon");
}

#[test]
fn choosing_a_mode_rewrites_the_operator_and_keeps_the_text() {
    assert_eq!(with_search_mode("radar", SearchMode::Content), "content: radar");
    assert_eq!(with_search_mode("file: radar", SearchMode::Content), "content: radar");
    assert_eq!(with_search_mode("content: radar", SearchMode::Files), "radar");
    assert_eq!(with_search_mode("", SearchMode::Content), "content: ");
}
