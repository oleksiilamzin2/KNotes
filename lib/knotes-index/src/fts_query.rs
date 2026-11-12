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
///
/// With `prefixes` (search as you type) short words are prefixes too: `"ба"*`.
pub(crate) fn query_groups(query: &str, stemmer: &impl Stemmer, prefixes: bool) -> Vec<QueryGroup> {
    let words: Vec<String> =
        query.split_whitespace().map(|raw| raw.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect();
    let mut groups = Vec::new();
    for word in words {
        let stem = stemmer.stem(&word);
        let mut expr = String::new();
        let short = word.chars().count() <= EXACT_MAX_CHARS;
        let typed = short && prefixes;
        if typed {
            let _ = write!(expr, "{}*", quoted(&word));
        } else if short {
            let _ = write!(expr, "{}", quoted(&word));
        } else if stem == word {
            let _ = write!(expr, "{}*", quoted(&word));
        } else {
            let _ = write!(expr, "({} OR {}*)", quoted(&word), quoted(stem));
        }
        groups.push(QueryGroup { word, expr, typed });
    }
    groups
}

/// One query word and its FTS5 expression.
pub(crate) struct QueryGroup {
    /// Lowercased word as typed, used to look up how frequent it is.
    pub word: String,
    pub expr: String,
    /// A short word turned into a prefix while typing: its exact frequency says nothing.
    pub typed: bool,
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
        let groups = query_groups(q, &LightStemmer, false);
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
