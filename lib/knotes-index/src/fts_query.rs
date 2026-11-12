//! Turns plain user text into a safe FTS5 MATCH expression with word forms (ADR 0008).

use std::fmt::Write as _;

use crate::stemmer::Stemmer;

/// Words up to this many characters are searched exactly (`і`, `та`, `the`).
const EXACT_MAX_CHARS: usize = 3;

/// Every word becomes a group, groups are joined with AND:
/// - short word: `"і"`;
/// - word without an ending: `"радар"*` (all forms);
/// - word with an ending: `("мовою" OR "мов"*)`: all forms, and the exact form matches twice,
///   so it ranks higher.
///
/// Words are quoted FTS5 phrases, so user input is never FTS syntax; FTS5 tokenizes inside the
/// quotes with the table tokenizer (`пам'ять` becomes `пам ять`). Punctuation around a word is
/// dropped, and so are words without letters or digits. `None` means nothing to search for.
pub(crate) fn match_expression(query: &str, stemmer: &impl Stemmer) -> Option<String> {
    let mut out = String::new();
    for raw in query.split_whitespace() {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if word.is_empty() {
            continue;
        }
        // Explicit AND: FTS5 has no implicit AND between parenthesized groups.
        if !out.is_empty() {
            out.push_str(" AND ");
        }
        let stem = stemmer.stem(&word);
        if word.chars().count() <= EXACT_MAX_CHARS {
            let _ = write!(out, "{}", quoted(&word));
        } else if stem == word {
            let _ = write!(out, "{}*", quoted(&word));
        } else {
            let _ = write!(out, "({} OR {}*)", quoted(&word), quoted(stem));
        }
    }
    (!out.is_empty()).then_some(out)
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::match_expression;
    use crate::light_stemmer::LightStemmer;

    fn expr(q: &str) -> Option<String> {
        match_expression(q, &LightStemmer)
    }

    #[test]
    fn groups_by_word_kind() {
        assert_eq!(expr("мовою").as_deref(), Some("(\"мовою\" OR \"мов\"*)"));
        assert_eq!(expr("Радар,").as_deref(), Some("\"радар\"*"));
        assert_eq!(expr("і the").as_deref(), Some("\"і\" AND \"the\""));
    }

    #[test]
    fn quotes_are_escaped_and_symbol_only_words_dropped() {
        assert_eq!(expr("say\"hi - (").as_deref(), Some("\"say\"\"hi\"*"));
        assert_eq!(expr(" - * "), None);
        assert_eq!(expr(""), None);
    }
}
