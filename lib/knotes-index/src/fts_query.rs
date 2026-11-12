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
/// dropped, and so are words without letters or digits.
pub(crate) fn query_groups(query: &str, stemmer: &impl Stemmer) -> Vec<QueryGroup> {
    let mut groups = Vec::new();
    for raw in query.split_whitespace() {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if word.is_empty() {
            continue;
        }
        let stem = stemmer.stem(&word);
        let mut expr = String::new();
        if word.chars().count() <= EXACT_MAX_CHARS {
            let _ = write!(expr, "{}", quoted(&word));
        } else if stem == word {
            let _ = write!(expr, "{}*", quoted(&word));
        } else {
            let _ = write!(expr, "({} OR {}*)", quoted(&word), quoted(stem));
        }
        groups.push(QueryGroup { word, expr });
    }
    groups
}

/// One query word and its FTS5 expression.
pub(crate) struct QueryGroup {
    /// Lowercased word as typed, used to look up how frequent it is.
    pub word: String,
    pub expr: String,
}

/// Explicit AND: FTS5 has no implicit AND between parenthesized groups.
pub(crate) fn join<'a>(groups: impl IntoIterator<Item = &'a QueryGroup>) -> String {
    groups.into_iter().map(|g| g.expr.as_str()).collect::<Vec<_>>().join(" AND ")
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::{join, query_groups};
    use crate::light_stemmer::LightStemmer;

    fn expr(q: &str) -> Option<String> {
        let groups = query_groups(q, &LightStemmer);
        (!groups.is_empty()).then(|| join(&groups))
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
