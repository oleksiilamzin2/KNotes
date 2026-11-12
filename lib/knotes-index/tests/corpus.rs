use knotes_core::{CorpusSource, FileKind, IndexChange, IndexWriter, NoteParser, TextCorpus, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = VaultFile { path: path.into(), kind: FileKind::from_path(path), size: source.len() as u64, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

fn notes(corpus: &impl TextCorpus) -> Vec<(String, String)> {
    (0..corpus.len())
        .map(|i| {
            let (p, t) = corpus.note(i);
            (p.to_owned(), String::from_utf8(t.to_vec()).unwrap())
        })
        .collect()
}

#[test]
fn corpus_has_notes_in_path_order_without_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "b.md", "second");
    upsert(&mut index, "a.md", "перша");
    let pdf = VaultFile { path: "c.pdf".into(), kind: FileKind::Pdf, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &pdf, note: None }]).unwrap();
    let corpus = index.corpus().unwrap();
    assert_eq!(notes(&*corpus), [("a.md".to_owned(), "перша".to_owned()), ("b.md".to_owned(), "second".to_owned())]);
}

#[test]
fn old_snapshot_keeps_old_texts_and_new_one_sees_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "a.md", "old");
    let before = index.corpus().unwrap();
    upsert(&mut index, "a.md", "new text");
    upsert(&mut index, "z.md", "added");
    let after = index.corpus().unwrap();
    assert_eq!(notes(&*before), [("a.md".to_owned(), "old".to_owned())]);
    assert_eq!(notes(&*after), [("a.md".to_owned(), "new text".to_owned()), ("z.md".to_owned(), "added".to_owned())]);
}

#[test]
fn empty_index_gives_empty_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert!(index.corpus().unwrap().is_empty());
}
