use knotes_core::{CatalogEntry, FileKind, IndexChange, IndexWriter, NoteCatalog, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn upsert(index: &mut SqliteIndex, path: &str, source: Option<&str>) {
    let f = VaultFile { path: path.into(), kind: FileKind::from_path(path), size: 1, mtime_ns: 1 };
    let parsed = source.map(|s| MarkdownParser::new().parse(s));
    let note = source.zip(parsed.as_ref());
    index.apply(&[IndexChange::Upsert { file: &f, note }]).unwrap();
}

fn entry(path: &str, aliases: &[&str]) -> CatalogEntry {
    CatalogEntry { path: path.into(), aliases: aliases.iter().map(|a| a.to_string()).collect() }
}

#[test]
fn catalog_lists_all_files_with_aliases_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "b.md", Some("---\naliases: [Бета, Second]\n---\ntext"));
    upsert(&mut index, "a.md", Some("plain"));
    upsert(&mut index, "att/p.pdf", None);
    assert_eq!(index.catalog().unwrap(), [entry("a.md", &[]), entry("att/p.pdf", &[]), entry("b.md", &["Бета", "Second"])]);
}

#[test]
fn aliases_follow_note_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "n.md", Some("---\naliases: [Old]\n---\n"));
    upsert(&mut index, "n.md", Some("---\naliases: [New]\n---\n"));
    assert_eq!(index.catalog().unwrap(), [entry("n.md", &["New"])]);
    index.apply(&[IndexChange::Remove { path: "n.md" }]).unwrap();
    assert!(index.catalog().unwrap().is_empty());
}
