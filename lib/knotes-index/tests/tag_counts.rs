use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, TagIndex, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

#[test]
fn counts_include_nested_tags_without_double_counting() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    for (path, text) in [("a.md", "#project/radar #project/ui #todo"), ("b.md", "#project/ui"), ("c.md", "#Project/Radar/fmcw")] {
        let parsed = MarkdownParser::new().parse(text);
        let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
        index.apply(&[IndexChange::Upsert { file: &f, note: Some((text, &parsed)) }]).unwrap();
    }
    let counts = index.tag_counts().unwrap();
    let expect = [("project", 3), ("project/radar", 2), ("project/radar/fmcw", 1), ("project/ui", 2), ("todo", 1)];
    assert_eq!(counts, expect.map(|(t, n)| (t.to_owned(), n)));
}

#[test]
fn untagged_notes_have_no_inline_or_frontmatter_tag() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    for (path, text) in [("a.md", "#todo here"), ("b.md", "no tags"), ("c.md", "---\ntags: [idea]\n---\nbody"), ("d.md", "")] {
        let parsed = MarkdownParser::new().parse(text);
        let f = VaultFile { path: path.into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
        index.apply(&[IndexChange::Upsert { file: &f, note: Some((text, &parsed)) }]).unwrap();
    }
    let png = VaultFile { path: "pic.png".into(), kind: FileKind::Other, size: 1, mtime_ns: 1 };
    index.apply(&[IndexChange::Upsert { file: &png, note: None }]).unwrap();
    assert_eq!(index.untagged_notes().unwrap(), ["b.md", "d.md"], "notes only, not attachments");
}
