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

#[test]
fn releasing_memory_empties_the_page_cache_a_large_write_filled() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let texts: Vec<String> = (0..3000).map(|i| format!("# Note {i}\n\n{}\n", "radar sonar lidar words ".repeat(40))).collect();
    let parsed: Vec<_> = texts.iter().map(|t| MarkdownParser::new().parse(t)).collect();
    let files: Vec<VaultFile> = (0..3000).map(|i| VaultFile { path: format!("n{i}.md"), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
    let changes: Vec<IndexChange> =
        files.iter().zip(&texts).zip(&parsed).map(|((f, t), p)| IndexChange::Upsert { file: f, note: Some((t, p)) }).collect();
    index.apply(&changes).unwrap();
    let filled = index.cache_used();
    assert!(filled > 1 << 20, "a large write fills the cache: {filled}");
    index.release_memory();
    let left = index.cache_used();
    assert!(left < filled / 4, "{filled} -> {left}");
}
