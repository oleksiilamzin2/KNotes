use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, NoteRow, NoteRows, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

#[test]
fn rows_have_mtime_and_snippet_for_notes_and_pdfs_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let text = "# Title\n\nFirst line of the body.\n";
    let parsed = MarkdownParser::new().parse(text);
    let note = VaultFile { path: "a.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 42 };
    let pdf = VaultFile { path: "b.pdf".into(), kind: FileKind::Pdf, size: 1, mtime_ns: 7 };
    let image = VaultFile { path: "c.png".into(), kind: FileKind::Image, size: 1, mtime_ns: 8 };
    index
        .apply(&[
            IndexChange::Upsert { file: &note, note: Some((text, &parsed)) },
            IndexChange::Upsert { file: &pdf, note: None },
            IndexChange::Upsert { file: &image, note: None },
        ])
        .unwrap();
    // PDFs are found by the search as entries of their own (ADR 0022).
    assert_eq!(
        index.note_rows().unwrap(),
        [NoteRow { path: "a.md".into(), mtime_ns: 42, snippet: "First line of the body.".into() }, NoteRow { path: "b.pdf".into(), mtime_ns: 7, snippet: String::new() }]
    );
}
