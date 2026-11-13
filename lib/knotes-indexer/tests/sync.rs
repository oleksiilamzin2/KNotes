use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use knotes_core::{FileStates, LinkGraph, NoteCatalog, NoteText};
use knotes_index::SqliteIndex;
use knotes_indexer::{Indexer, SyncReport};
use knotes_md::MarkdownParser;
use knotes_vault::FsVault;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn sync(vault: &Path, index: &mut SqliteIndex) -> SyncReport {
    Indexer::new(FsVault::new(vault), MarkdownParser::new()).sync(index).unwrap()
}

#[test]
fn first_sync_indexes_everything_with_resolved_links() {
    let (vault, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(vault.path(), "a.md", "[[Z note]] and ![[pic.png]]");
    write(vault.path(), "deep/Z note.md", "target");
    write(vault.path(), "att/pic.png", "png");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    let report = sync(vault.path(), &mut index);
    assert_eq!((report.added, report.modified, report.removed), (3, 0, 0));
    assert!(report.failed.is_empty());
    assert_eq!(index.catalog().unwrap().len(), 3);
    assert_eq!(index.note_text("deep/Z note.md").unwrap().as_deref(), Some("target"));
    let targets: Vec<_> = index.links_from("a.md").unwrap().into_iter().map(|l| l.resolved).collect();
    assert_eq!(targets, [Some("deep/Z note.md".into()), Some("att/pic.png".into())]);
}

#[test]
fn unchanged_vault_needs_no_work() {
    let (vault, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(vault.path(), "a.md", "text");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    sync(vault.path(), &mut index);
    let again = sync(vault.path(), &mut index);
    assert_eq!((again.added, again.modified, again.removed), (0, 0, 0));
}

#[test]
fn changed_and_deleted_files_are_picked_up() {
    let (vault, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(vault.path(), "a.md", "first version");
    write(vault.path(), "gone.md", "bye");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    sync(vault.path(), &mut index);
    write(vault.path(), "a.md", "second, longer version");
    fs::remove_file(vault.path().join("gone.md")).unwrap();
    let report = sync(vault.path(), &mut index);
    assert_eq!((report.added, report.modified, report.removed), (0, 1, 1));
    assert_eq!(index.note_text("a.md").unwrap().as_deref(), Some("second, longer version"));
    assert_eq!(index.file_states().unwrap().len(), 1);
}

#[test]
fn unreadable_note_is_reported_and_retried_next_time() {
    let (vault, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(vault.path(), "locked.md", "secret");
    let locked = vault.path().join("locked.md");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    let report = sync(vault.path(), &mut index);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(index.note_text("locked.md").unwrap(), None);

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    let report = sync(vault.path(), &mut index);
    assert!(report.failed.is_empty());
    assert_eq!(report.modified, 1);
    assert_eq!(index.note_text("locked.md").unwrap().as_deref(), Some("secret"));
}

/// Text for PDFs, as the app's PDFKit reader gives it.
struct Texts;

impl knotes_core::DocumentText for Texts {
    fn document_text(&self, path: &str) -> Option<String> {
        path.ends_with(".pdf").then(|| format!("Title of {path}\n\nA #word and [[no link]] in the second paragraph."))
    }
}

#[test]
fn a_pdf_s_text_is_indexed_and_found_as_plain_text() {
    use knotes_core::{ContentQuery, ContentSearch, TagIndex, Within};
    let (vault, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(vault.path(), "a.md", "a note");
    write(vault.path(), "docs/paper.pdf", "%PDF-1.4");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(vault.path()), MarkdownParser::new(), Texts).sync(&mut index).unwrap();
    assert!(index.note_text("docs/paper.pdf").unwrap().unwrap().contains("second paragraph"));
    let query = ContentQuery { text: "paragraph", within: Within::Note, prefixes: false, scope: None, limit: 10 };
    let hits: Vec<String> = index.search(&query).unwrap().into_iter().map(|h| h.path).collect();
    assert_eq!(hits, ["docs/paper.pdf"]);
    assert!(index.tag_counts().unwrap().is_empty(), "no tags from a PDF's text");
    assert!(index.links_from("docs/paper.pdf").unwrap().is_empty(), "no links either");
}
