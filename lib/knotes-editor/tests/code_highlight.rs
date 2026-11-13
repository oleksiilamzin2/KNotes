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

use knotes_editor::CodeHighlighter;

#[test]
fn a_highlighter_kept_for_a_block_parses_again_only_the_lines_an_edit_changes() {
    let mut highlighter = CodeHighlighter::new("rust").unwrap();
    let code: String = (0..50).map(|i| format!("let v{i} = {i}; // n\n")).collect();
    let (tokens, parsed) = highlighter.update(&code);
    assert_eq!((tokens, parsed), (highlight_code("rust", &code).unwrap(), 50));
    let edited = code.replacen("let v20 = 20;", "let v20 = 2000;", 1);
    let (tokens, parsed) = highlighter.update(&edited);
    assert_eq!(tokens, highlight_code("rust", &edited).unwrap());
    assert_eq!(parsed, 1, "only the line typed in");
    // A comment opened: every line after it changes, and is parsed again.
    let opened = edited.replacen("let v30", "/* let v30", 1);
    let (tokens, parsed) = highlighter.update(&opened);
    assert_eq!(tokens, highlight_code("rust", &opened).unwrap());
    assert_eq!(parsed, 20);
}

#[test]
fn a_highlighter_follows_lines_added_and_removed() {
    let mut highlighter = CodeHighlighter::new("python").unwrap();
    let code = "def a():\n    return 1\n";
    highlighter.update(code);
    for next in ["def a():\n    x = 2\n    return 1\n", "def a():\n", "", "s = '''\nmore\n'''\n"] {
        assert_eq!(highlighter.update(next).0, highlight_code("python", next).unwrap(), "{next:?}");
    }
}
