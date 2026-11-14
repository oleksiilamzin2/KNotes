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

#[test]
fn a_file_s_mtime_is_looked_up_by_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let note = VaultFile { path: "a.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 42 };
    let pdf = VaultFile { path: "b.pdf".into(), kind: FileKind::Pdf, size: 1, mtime_ns: 7 };
    index.apply(&[IndexChange::Upsert { file: &note, note: None }, IndexChange::Upsert { file: &pdf, note: None }]).unwrap();
    assert_eq!((index.modified("a.md").unwrap(), index.modified("b.pdf").unwrap(), index.modified("none.md").unwrap()), (Some(42), Some(7), None));
}

#[test]
fn a_note_s_blocks_come_in_order_with_their_place_and_heading() {
    use knotes_core::{BlockHit, NoteBlocks};
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let text = "Intro line.\n\n# Boiler\n\nReplace the anode.\n";
    let parsed = MarkdownParser::new().parse(text);
    let note = VaultFile { path: "a.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &note, note: Some((text, &parsed)) }]).unwrap();
    let blocks = index.note_blocks("a.md").unwrap();
    let expected: Vec<BlockHit> = parsed
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| BlockHit { path: "a.md".into(), ord: i as u32, span: b.span, heading: b.heading.map(|h| parsed.headings[h as usize].text.clone()), score: 0.0 })
        .collect();
    assert_eq!(blocks, expected);
    assert!(blocks.iter().any(|b| b.heading.as_deref() == Some("Boiler") && text[b.span.start as usize..b.span.end as usize].contains("Replace the anode.")), "{blocks:?}");
    assert_eq!(index.note_blocks("none.md").unwrap(), Vec::<BlockHit>::new());
}
