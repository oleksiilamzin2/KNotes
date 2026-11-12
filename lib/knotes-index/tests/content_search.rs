use knotes_core::{ContentSearch, FileKind, IndexChange, IndexWriter, NoteParser, NoteText, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn index_with(notes: &[(&str, &str)]) -> (tempfile::TempDir, SqliteIndex) {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    for (path, source) in notes {
        upsert(&mut index, path, source);
    }
    (dir, index)
}

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: source.len() as u64, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

fn paths(index: &SqliteIndex, query: &str) -> Vec<String> {
    index.search_blocks(query, None, 50).unwrap().into_iter().map(|h| h.path).collect()
}

#[test]
fn finds_the_block_and_its_span_and_heading() {
    let src = "intro text\n\n## Signals\n\nThe radar emits a chirp.\n";
    let (_d, index) = index_with(&[("a.md", src), ("b.md", "nothing here\n")]);
    let hits = index.search_blocks("chirp", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    let h = &hits[0];
    assert_eq!((h.path.as_str(), h.ord, h.heading.as_deref()), ("a.md", 1, Some("Signals")));
    let text = index.note_text("a.md").unwrap().unwrap();
    assert!(text[h.span.start as usize..h.span.end as usize].contains("chirp"));
}

#[test]
fn all_words_must_match() {
    let (_d, index) = index_with(&[("both.md", "radar signal\n"), ("one.md", "radar only\n")]);
    assert_eq!(paths(&index, "signal radar"), ["both.md"]);
}

#[test]
fn better_matches_rank_first() {
    let (_d, index) = index_with(&[
        ("weak.md", "radar appears once in a long sentence about many other unrelated things\n"),
        ("strong.md", "radar radar radar\n"),
    ]);
    assert_eq!(paths(&index, "radar"), ["strong.md", "weak.md"]);
}

#[test]
fn ukrainian_is_case_insensitive_and_keeps_letters_distinct() {
    let (_d, index) = index_with(&[("uk.md", "Радар і йога\n"), ("en.md", "café\n")]);
    assert_eq!(paths(&index, "радар"), ["uk.md"]);
    assert!(paths(&index, "иога").is_empty(), "й and и are different letters");
    assert_eq!(paths(&index, "cafe"), ["en.md"], "Latin diacritics are folded");
}

#[test]
fn apostrophe_words_match() {
    let (_d, index) = index_with(&[("a.md", "моя пам'ять\n"), ("b.md", "пам та ять окремо\n")]);
    assert_eq!(paths(&index, "пам'ять"), ["a.md"]);
}

#[test]
fn updated_and_removed_notes_leave_the_index() {
    let (_d, mut index) = index_with(&[("n.md", "old words\n")]);
    upsert(&mut index, "n.md", "fresh words\n");
    assert!(paths(&index, "old").is_empty());
    assert_eq!(paths(&index, "fresh"), ["n.md"]);
    index.apply(&[IndexChange::Remove { path: "n.md" }]).unwrap();
    assert!(paths(&index, "fresh").is_empty());
}

#[test]
fn query_syntax_characters_are_plain_text() {
    let (_d, index) = index_with(&[("a.md", "alpha beta\n")]);
    for q in ["\"alpha", "alpha AND", "NOT beta", "(alpha", "alpha*", "-", "", "  "] {
        assert!(index.search_blocks(q, None, 10).is_ok(), "query {q:?} failed");
    }
    assert_eq!(paths(&index, "alpha AND"), Vec::<String>::new(), "AND is a word that must occur");
}

#[test]
fn limit_is_respected() {
    let notes: Vec<(String, String)> = (0..20).map(|i| (format!("n{i}.md"), "common word\n".to_owned())).collect();
    let refs: Vec<(&str, &str)> = notes.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    let (_d, index) = index_with(&refs);
    assert_eq!(index.search_blocks("common", None, 5).unwrap().len(), 5);
}

#[test]
fn ukrainian_word_forms_match() {
    let (_d, index) = index_with(&[("a.md", "рідна мова\n"), ("b.md", "дві мови\n"), ("c.md", "розмова мовою\n"), ("d.md", "інше\n")]);
    let mut found = paths(&index, "мовою");
    found.sort();
    assert_eq!(found, ["a.md", "b.md", "c.md"]);
}

#[test]
fn exact_form_ranks_above_other_forms() {
    let (_d, index) = index_with(&[("other.md", "про радари тут\n"), ("exact.md", "про радарами тут\n")]);
    assert_eq!(paths(&index, "радарами"), ["exact.md", "other.md"]);
}

#[test]
fn english_word_forms_match() {
    let (_d, index) = index_with(&[("a.md", "we searched everything\n"), ("b.md", "searches run\n")]);
    let mut found = paths(&index, "searching");
    found.sort();
    assert_eq!(found, ["a.md", "b.md"]);
}

#[test]
fn short_words_match_exactly_and_punctuation_is_ignored() {
    let (_d, index) = index_with(&[("a.md", "кава і чай\n"), ("b.md", "кава із чаєм\n")]);
    assert_eq!(paths(&index, "і"), ["a.md"]);
    assert_eq!(paths(&index, "кава,").len(), 2);
}

#[test]
fn several_inflected_words_match_together() {
    let (_d, index) = index_with(&[("a.md", "робота з системою даних\n"), ("b.md", "система без нічого\n")]);
    assert_eq!(paths(&index, "системами даних"), ["a.md"]);
}

#[test]
fn scope_limits_results_to_given_notes() {
    let (_d, index) = index_with(&[("a.md", "radar here\n"), ("b.md", "radar there\n"), ("c.md", "radar too\n")]);
    let scope = vec!["b.md".to_owned(), "c.md".to_owned(), "missing.md".to_owned()];
    let mut found: Vec<String> = index.search_blocks("radar", Some(&scope), 10).unwrap().into_iter().map(|h| h.path).collect();
    found.sort();
    assert_eq!(found, ["b.md", "c.md"]);
    assert!(index.search_blocks("radar", Some(&[]), 10).unwrap().is_empty());
}
