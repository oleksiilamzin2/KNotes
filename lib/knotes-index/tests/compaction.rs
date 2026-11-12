use knotes_core::{CorpusSource, FileKind, FileStates, IndexChange, IndexWriter, NoteParser, NoteText, TextCorpus, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: source.len() as u64, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

fn pack_len(dir: &std::path::Path) -> u64 {
    std::fs::metadata(dir.join("content.pack")).unwrap().len()
}

#[test]
fn compaction_drops_dead_versions_and_keeps_texts() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let big = "radar ".repeat(2000);
    for i in 0..20 {
        upsert(&mut index, "edited.md", &format!("{big} version {i}"));
    }
    upsert(&mut index, "stable.md", "stays the same");
    let before = pack_len(dir.path());
    assert!(index.compact_pack().unwrap());
    assert!(pack_len(dir.path()) < before / 10, "{} -> {}", before, pack_len(dir.path()));
    assert_eq!(index.note_text("edited.md").unwrap().unwrap(), format!("{big} version 19"));
    assert_eq!(index.note_text("stable.md").unwrap().as_deref(), Some("stays the same"));
    upsert(&mut index, "after.md", "written after compaction");
    assert_eq!(index.note_text("after.md").unwrap().as_deref(), Some("written after compaction"));
}

#[test]
fn snapshot_taken_before_compaction_still_reads_its_texts() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "a.md", "old text");
    upsert(&mut index, "a.md", "current text");
    let snapshot = index.corpus().unwrap();
    index.compact_pack().unwrap();
    assert_eq!(snapshot.note(0).1, b"current text");
}

#[test]
fn compacted_index_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        upsert(&mut index, "a.md", "one");
        upsert(&mut index, "a.md", "two");
        index.compact_pack().unwrap();
    }
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert_eq!(index.note_text("a.md").unwrap().as_deref(), Some("two"));
}

#[test]
fn pack_from_another_generation_resets_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("stale.pack");
    {
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        upsert(&mut index, "a.md", "text before");
        std::fs::copy(dir.path().join("content.pack"), &stale).unwrap();
        upsert(&mut index, "a.md", "text after");
        index.compact_pack().unwrap();
    }
    // As if a crash left the pre-compaction file in place.
    std::fs::rename(&stale, dir.path().join("content.pack")).unwrap();
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert!(index.file_states().unwrap().is_empty());
}
