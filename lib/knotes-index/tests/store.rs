use knotes_core::{
    FileKind, FileState, FileStates, IndexChange, IndexWriter, LinkGraph, LinkKind, NoteOutline, NoteParser, TagIndex,
    VaultFile,
};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn file(path: &str, size: u64, mtime_ns: i64) -> VaultFile {
    VaultFile { path: path.into(), kind: FileKind::from_path(path), size, mtime_ns }
}

fn upsert(index: &mut SqliteIndex, path: &str, source: &str) {
    let parsed = MarkdownParser::new().parse(source);
    let f = file(path, source.len() as u64, 1);
    index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
}

#[test]
fn reopening_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut index = SqliteIndex::open(dir.path()).unwrap();
        upsert(&mut index, "a.md", "# A\n");
    }
    let index = SqliteIndex::open(dir.path()).unwrap();
    assert_eq!(index.file_states().unwrap().len(), 1);
}

#[test]
fn stores_headings_links_and_tags_of_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let src = "---\ntags: [Project/Radar]\n---\n# Title\n\nSee [[Other#Part|it]] and [site](https://x.y). #Todo #todo\n\n## Next\n";
    upsert(&mut index, "Folder/Note.md", src);

    let headings = index.headings("Folder/Note.md").unwrap();
    assert_eq!(headings.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(), [(1, "Title"), (2, "Next")]);

    let links = index.links_from("Folder/Note.md").unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!((links[0].kind, links[0].target.as_str()), (LinkKind::Wiki, "Other"));
    assert_eq!((links[0].subpath.as_deref(), links[0].display.as_deref()), (Some("Part"), Some("it")));
    assert_eq!(links[0].span, MarkdownParser::new().parse(src).links[0].span);
    assert_eq!(links[1].kind, LinkKind::External);

    assert_eq!(index.tags_of("Folder/Note.md").unwrap(), ["project/radar", "todo"]);
}

#[test]
fn nested_tag_query_matches_children_only_by_segment() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "a.md", "#area/sub\n");
    upsert(&mut index, "b.md", "#area\n");
    upsert(&mut index, "c.md", "#areas\n");
    assert_eq!(index.notes_with_tag("Area").unwrap(), ["a.md", "b.md"]);
    assert_eq!(index.notes_with_tag("area/sub").unwrap(), ["a.md"]);
}

#[test]
fn upsert_replaces_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "n.md", "# Old\n[[A]] #old\n");
    upsert(&mut index, "n.md", "# New\n");
    assert_eq!(index.headings("n.md").unwrap()[0].text, "New");
    assert!(index.links_from("n.md").unwrap().is_empty());
    assert!(index.tags_of("n.md").unwrap().is_empty());
    assert_eq!(index.file_states().unwrap().len(), 1);
}

#[test]
fn remove_deletes_the_note_and_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    upsert(&mut index, "n.md", "# H\n[[A]] #t\n");
    index.apply(&[IndexChange::Remove { path: "n.md" }]).unwrap();
    assert!(index.file_states().unwrap().is_empty());
    assert!(index.headings("n.md").unwrap().is_empty());
    assert!(index.notes_with_tag("t").unwrap().is_empty());
}

#[test]
fn attachments_are_files_without_note_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let pdf = file("att/Paper.pdf", 1000, 42);
    index.apply(&[IndexChange::Upsert { file: &pdf, note: None }]).unwrap();
    assert_eq!(index.file_states().unwrap(), [FileState { path: "att/Paper.pdf".into(), size: 1000, mtime_ns: 42 }]);
}

#[test]
fn a_failed_batch_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let ok = file("ok.md", 1, 1);
    let bad = VaultFile { size: u64::MAX, ..file("bad.md", 1, 1) };
    let result = index.apply(&[IndexChange::Upsert { file: &ok, note: None }, IndexChange::Upsert { file: &bad, note: None }]);
    assert!(result.is_err());
    assert!(index.file_states().unwrap().is_empty());
}
