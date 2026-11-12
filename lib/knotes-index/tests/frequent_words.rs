//! Words found in most blocks (і, the) carry no ranking signal and make bm25 rank the whole
//! index; on large vaults they are dropped from queries that have other words.

use knotes_core::{ContentSearch, FileKind, IndexChange, IndexWriter, NoteParser, ParsedNote, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn index_with(texts: &[String]) -> (tempfile::TempDir, SqliteIndex) {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let files: Vec<VaultFile> = (0..texts.len())
        .map(|i| VaultFile { path: format!("n{i:05}.md"), kind: FileKind::Markdown, size: 1, mtime_ns: 1 })
        .collect();
    let parsed: Vec<ParsedNote> = texts.iter().map(|t| MarkdownParser::new().parse(t)).collect();
    let changes: Vec<IndexChange> =
        (0..texts.len()).map(|i| IndexChange::Upsert { file: &files[i], note: Some((texts[i].as_str(), &parsed[i])) }).collect();
    index.apply(&changes).unwrap();
    (dir, index)
}

/// `n` notes that all contain "і"; the last one has "радар" without "і", one more has both.
fn vault(n: usize) -> Vec<String> {
    let mut texts: Vec<String> = (0..n - 2).map(|i| format!("слово{i} і ще\n")).collect();
    texts.push("радар і антена\n".into());
    texts.push("радар без сполучника\n".into());
    texts
}

#[test]
fn frequent_word_is_dropped_when_other_words_remain() {
    let (_d, index) = index_with(&vault(2500));
    let mut found: Vec<String> = index.search_blocks("і радар", 10).unwrap().into_iter().map(|h| h.path).collect();
    found.sort();
    assert_eq!(found, ["n02498.md", "n02499.md"]);
}

#[test]
fn query_of_only_frequent_words_still_returns_blocks() {
    let (_d, index) = index_with(&vault(2500));
    assert_eq!(index.search_blocks("і", 10).unwrap().len(), 10);
}

#[test]
fn small_vaults_keep_every_word() {
    let (_d, index) = index_with(&vault(50));
    let found: Vec<String> = index.search_blocks("і радар", 10).unwrap().into_iter().map(|h| h.path).collect();
    assert_eq!(found, ["n00048.md"]);
}
