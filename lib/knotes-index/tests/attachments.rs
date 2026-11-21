use knotes_core::{Attachment, AttachmentIndex, FileKind, IndexChange, IndexWriter, MissingAttachment, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

fn put(index: &mut SqliteIndex, path: &str, source: Option<&str>) {
    let file = VaultFile { path: path.into(), kind: FileKind::from_path(path), size: 7, mtime_ns: 1 };
    let parsed = source.map(|source| MarkdownParser::new().parse(source));
    index.apply(&[IndexChange::Upsert { file: &file, note: source.zip(parsed.as_ref()) }]).unwrap();
}

#[test]
fn attachments_are_the_files_that_are_no_notes_with_the_links_to_each() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    put(&mut index, "pic.png", None);
    put(&mut index, "docs/paper.pdf", None);
    put(&mut index, "lone.jpg", None);
    put(&mut index, "board.canvas", None);
    put(&mut index, "a.md", Some("![[pic.png]] and [[paper.pdf]] and ![[gone.png]] and [[No note]] and ![[gone.png|30]]"));
    put(&mut index, "b.md", Some("![[pic.png]] [site](https://example.com/x.png) ![](lost/scan.pdf)"));
    let file = |path: &str, links| Attachment { path: path.to_owned(), size: 7, links };
    assert_eq!(index.attachments().unwrap(), [file("docs/paper.pdf", 1), file("lone.jpg", 0), file("pic.png", 2)], "no note, no canvas");
    let missing = |target: &str, source: &str| MissingAttachment { target: target.to_owned(), source: source.to_owned() };
    assert_eq!(index.missing_attachments().unwrap(), [missing("gone.png", "a.md"), missing("lost/scan.pdf", "b.md")], "no missing note, nothing of the web");
}
