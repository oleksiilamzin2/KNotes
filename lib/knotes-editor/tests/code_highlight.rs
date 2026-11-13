use knotes_editor::{CodeToken, highlight_code};

/// The text of each token of `kind`.
fn tokens<'a>(code: &'a str, language: &str, kind: CodeToken) -> Vec<&'a str> {
    highlight_code(language, code).unwrap().into_iter().filter(|(_, k)| *k == kind).map(|(r, _)| &code[r]).collect()
}

const RUST: &str = "fn main() {\n    let x = 42; // the answer\n    println!(\"hi\");\n}\n";

#[test]
fn rust_code_is_split_into_keywords_numbers_comments_strings_and_functions() {
    assert!(tokens(RUST, "rust", CodeToken::Keyword).contains(&"fn"));
    assert!(tokens(RUST, "rust", CodeToken::Keyword).contains(&"let"));
    assert_eq!(tokens(RUST, "rust", CodeToken::Number), ["42"]);
    assert_eq!(tokens(RUST, "rust", CodeToken::Comment), ["// the answer\n"]);
    assert!(tokens(RUST, "rust", CodeToken::String).iter().any(|s| s.contains("hi")));
    assert!(tokens(RUST, "rust", CodeToken::Function).contains(&"main"));
}

#[test]
fn a_language_is_named_or_given_by_its_extension_in_any_case() {
    for language in ["rust", "rs", "Rust", "rust ignore", "{.rust}"] {
        assert!(highlight_code(language, RUST).is_some(), "{language}");
    }
}

#[test]
fn bat_s_grammars_cover_what_syntect_s_own_leave_out() {
    for (language, code) in [("typescript", "const a: number = 1;"), ("ts", "let b = 'x';"), ("swift", "let a = 1"), ("toml", "a = 1"), ("kotlin", "val a = 1"), ("dockerfile", "FROM rust")] {
        let tokens = highlight_code(language, code).unwrap_or_else(|| panic!("{language}"));
        assert!(!tokens.is_empty(), "{language}: {tokens:?}");
    }
}

#[test]
fn an_unknown_or_missing_language_is_not_highlighted() {
    assert!(highlight_code("nosuchlanguage", "x").is_none());
    assert!(highlight_code("", "x").is_none());
}

#[test]
fn tokens_are_in_order_and_do_not_overlap() {
    let tokens = highlight_code("rust", RUST).unwrap();
    assert!(tokens.windows(2).all(|w| w[0].0.end <= w[1].0.start), "{tokens:?}");
}
