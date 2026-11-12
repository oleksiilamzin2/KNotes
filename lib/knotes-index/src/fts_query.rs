//! Turns plain user text into a safe FTS5 MATCH expression.

/// Each whitespace-separated word becomes a quoted FTS5 phrase, so no user input is ever
/// FTS syntax. FTS5 tokenizes inside the quotes with the table tokenizer: `пам'ять` becomes
/// the phrase `пам ять`. Phrases are AND-ed. Words without letters or digits are dropped;
/// `None` means nothing is left to search for.
pub(crate) fn match_expression(query: &str) -> Option<String> {
    let mut out = String::new();
    for word in query.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push('"');
        out.push_str(&word.replace('"', "\"\""));
        out.push('"');
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::match_expression;

    #[test]
    fn words_become_quoted_phrases() {
        assert_eq!(match_expression("radar  signal").as_deref(), Some("\"radar\" \"signal\""));
        assert_eq!(match_expression("пам'ять").as_deref(), Some("\"пам'ять\""));
    }

    #[test]
    fn quotes_are_escaped_and_symbol_only_words_dropped() {
        assert_eq!(match_expression("say\"hi - (").as_deref(), Some("\"say\"\"hi\""));
        assert_eq!(match_expression(" - * "), None);
        assert_eq!(match_expression(""), None);
    }
}
