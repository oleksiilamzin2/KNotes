use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, NoteRow, NoteRows, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

#[test]
fn rows_have_mtime_and_snippet_for_notes_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let text = "# Title\n\nFirst line of the body.\n";
    let parsed = MarkdownParser::new().parse(text);
    let note = VaultFile { path: "a.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 42 };
    let pdf = VaultFile { path: "b.pdf".into(), kind: FileKind::Pdf, size: 1, mtime_ns: 7 };
    index
        .apply(&[IndexChange::Upsert { file: &note, note: Some((text, &parsed)) }, IndexChange::Upsert { file: &pdf, note: None }])
        .unwrap();
    assert_eq!(index.note_rows().unwrap(), [NoteRow { path: "a.md".into(), mtime_ns: 42, snippet: "First line of the body.".into() }]);
}
