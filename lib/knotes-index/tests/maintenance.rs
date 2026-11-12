use knotes_core::{ContentSearch, FileKind, IndexChange, IndexWriter, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

#[test]
fn maintenance_merges_and_checkpoints_without_changing_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    for i in 0..60 {
        let text = format!("note number{i} about radar\n");
        let parsed = MarkdownParser::new().parse(&text);
        let f = VaultFile { path: format!("n{i}.md"), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
        index.apply(&[IndexChange::Upsert { file: &f, note: Some((&text, &parsed)) }]).unwrap();
    }
    let before = index.search_blocks("radar", None, 100).unwrap().len();
    let report = index.maintenance().unwrap();
    assert!(report.merge_steps > 0, "{report:?}");
    assert_eq!(report.wal_frames_left, 0, "{report:?}");
    assert_eq!(index.search_blocks("radar", None, 100).unwrap().len(), before);
    assert_eq!(index.search_blocks("number7", None, 10).unwrap().len(), 1);
}
