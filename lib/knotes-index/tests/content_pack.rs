use knotes_core::{FileKind, FileStates, IndexChange, IndexWriter, NoteParser, NoteText, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: source.len() as u64, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

#[test]
fn note_text_round_trips_through_the_pack() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "a.md", "# A\n\nfirst");
    upsert(&mut index, "b.md", "Нотатка про радар ЧМ\n");
    assert_eq!(index.note_text("a.md").unwrap().as_deref(), Some("# A\n\nfirst"));
    assert_eq!(index.note_text("b.md").unwrap().as_deref(), Some("Нотатка про радар ЧМ\n"));
    assert_eq!(index.note_text("missing.md").unwrap(), None);
}

#[test]
fn new_version_replaces_old_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "n.md", "old text");
    upsert(&mut index, "n.md", "new");
    assert_eq!(index.note_text("n.md").unwrap().as_deref(), Some("new"));
}

#[test]
fn attachments_and_removed_notes_have_no_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let pdf = VaultFile { path: "p.pdf".into(), kind: FileKind::Pdf, size: 3, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &pdf, note: None }]).unwrap();
    upsert(&mut index, "n.md", "text");
    index.apply(&[IndexChange::Remove { path: "n.md" }]).unwrap();
    assert_eq!(index.note_text("p.pdf").unwrap(), None);
    assert_eq!(index.note_text("n.md").unwrap(), None);
}

#[test]
fn text_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        upsert(&mut index, "a.md", "kept");
    }
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert_eq!(index.note_text("a.md").unwrap().as_deref(), Some("kept"));
}

#[test]
fn many_notes_in_one_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let texts: Vec<String> = (0..200).map(|i| format!("note {i} ").repeat(i % 7 + 1)).collect();
    let files: Vec<VaultFile> = (0..200)
        .map(|i| VaultFile { path: format!("n{i}.md"), kind: FileKind::Markdown, size: 1, mtime_ns: 1 })
        .collect();
    let parsed: Vec<_> = texts.iter().map(|t| MarkdownParser::new().parse(t)).collect();
    let changes: Vec<IndexChange> =
        (0..200).map(|i| IndexChange::Upsert { file: &files[i], note: Some((texts[i].as_str(), &parsed[i])) }).collect();
    index.apply(&changes).unwrap();
    for i in [0, 99, 199] {
        assert_eq!(index.note_text(&format!("n{i}.md")).unwrap().as_deref(), Some(texts[i].as_str()));
    }
}

#[test]
fn truncated_pack_resets_the_index() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        upsert(&mut index, "a.md", "some text that will be cut");
    }
    let pack = dir.path().join("content.pack");
    let len = std::fs::metadata(&pack).unwrap().len();
    std::fs::OpenOptions::new().write(true).open(&pack).unwrap().set_len(len / 2).unwrap();
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert!(index.file_states().unwrap().is_empty(), "index must be rebuilt from the vault");
}
