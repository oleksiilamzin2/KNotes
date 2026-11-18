use std::sync::atomic::AtomicBool;

use knotes_assistant::connections::gather;
use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, VaultFile};
use knotes_index::{LinkTargets, SqliteIndex};

const NOTES: [(&str, &str); 5] = [
    ("Radar.md", "# Radar\n\nThe project.\n"),
    ("Plan.md", "First line.\n\nSee [[Radar]] for the budget, and [[Radar#Top|again]].\n"),
    ("Work/Notes.md", "The radar is late. Radar again.\n"),
    ("Both.md", "[[Radar]] links, and radar is named too.\n"),
    ("Other.md", "Nothing of it here.\n"),
];

/// A vault of `NOTES` on the disk and its index.
fn vault() -> (tempfile::TempDir, tempfile::TempDir, LinkTargets) {
    let (vault, index_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for (path, text) in NOTES {
        let file = vault.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let mut index = SqliteIndex::open(index_dir.path()).unwrap();
    let parsed: Vec<_> = NOTES.iter().map(|(_, t)| knotes_md::MarkdownParser::new().parse(t)).collect();
    let files: Vec<_> = NOTES.iter().map(|(p, _)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
    let changes: Vec<_> = files.iter().zip(NOTES).zip(&parsed).map(|((file, (_, text)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
    index.apply(&changes).unwrap();
    (vault, index_dir, LinkTargets::new(NOTES.iter().map(|(path, _)| *path)))
}

#[test]
fn a_document_s_connections_are_the_notes_linking_to_it_and_those_naming_it() {
    let (vault, index_dir, links) = vault();
    let (linked, unlinked) = gather(vault.path(), index_dir.path(), "Radar.md", "Radar", &links, &AtomicBool::new(false)).unwrap();
    let said = |found: &[(String, Vec<knotes_app::Mention>)]| found.iter().map(|(path, mentions)| format!("{path} {}", mentions.len())).collect::<Vec<_>>().join(", ");
    assert_eq!(said(&linked), "Both.md 1, Plan.md 2", "each note once, with each of its links");
    assert!(linked[1].1[0].line.contains("for the budget"), "the line around the link: {:?}", linked[1].1[0]);
    // Named without a link, whatever the case; a note's own name in itself is no mention, and
    // a name inside a link is the link.
    assert_eq!(said(&unlinked), "Both.md 1, Work/Notes.md 2");
    // A note gone from the disk since the index saw it is passed over.
    std::fs::remove_file(vault.path().join("Plan.md")).unwrap();
    let (linked, _) = gather(vault.path(), index_dir.path(), "Radar.md", "Radar", &links, &AtomicBool::new(false)).unwrap();
    assert_eq!(said(&linked), "Both.md 1");
    // Called off, or with no index to ask: nothing.
    assert!(gather(vault.path(), index_dir.path(), "Radar.md", "Radar", &links, &AtomicBool::new(true)).is_none());
    assert!(gather(vault.path(), &vault.path().join("no-index"), "Radar.md", "Radar", &links, &AtomicBool::new(false)).is_none());
    // A document nothing links to or names.
    let (linked, unlinked) = gather(vault.path(), index_dir.path(), "Other.md", "Other", &links, &AtomicBool::new(false)).unwrap();
    assert!(linked.is_empty() && unlinked.is_empty());
}
