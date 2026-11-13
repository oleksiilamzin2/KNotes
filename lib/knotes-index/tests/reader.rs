use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, NoteText, TagIndex, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

#[test]
fn reader_sees_what_the_writer_commits() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut writer, "a.md", "first\n#radar");
    let reader = SqliteIndex::open_reader(dir.path()).unwrap();
    assert_eq!(reader.notes_with_tag("radar").unwrap(), ["a.md"]);
    upsert(&mut writer, "b.md", "second\n#radar");
    assert_eq!(reader.notes_with_tag("radar").unwrap(), ["a.md", "b.md"]);
    assert_eq!(reader.note_text("b.md").unwrap().as_deref(), Some("second\n#radar"));
}

#[test]
fn reader_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    drop(SqliteIndex::open(dir.path()).unwrap());
    let mut reader = SqliteIndex::open_reader(dir.path()).unwrap();
    let f = VaultFile { path: "x.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
    assert!(reader.apply(&[IndexChange::Upsert { file: &f, note: None }]).is_err());
}

#[test]
fn reader_never_creates_an_index() {
    let dir = tempfile::tempdir().unwrap();
    assert!(SqliteIndex::open_reader(&dir.path().join("missing")).is_err());
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn reader_follows_pack_compaction_and_new_versions() {
    use knotes_core::ContentSearch;
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut writer, "a.md", "old radar");
    let reader = SqliteIndex::open_reader(dir.path()).unwrap();
    assert_eq!(reader.note_text("a.md").unwrap().as_deref(), Some("old radar"));
    assert_eq!(reader.search_blocks("radar", None, 10).unwrap().len(), 1);
    upsert(&mut writer, "a.md", "new text");
    writer.compact_pack().unwrap();
    assert_eq!(reader.note_text("a.md").unwrap().as_deref(), Some("new text"));
    assert!(reader.search_blocks("radar", None, 10).unwrap().is_empty());
    assert_eq!(reader.search_blocks("text", Some(&["a.md".to_owned()]), 10).unwrap().len(), 1);
}
