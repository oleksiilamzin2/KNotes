use knotes_core::{FileKind, IndexChange, IndexWriter, LinkGraph, NoteParser, VaultFile};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;

struct Vault {
    _dir: tempfile::TempDir,
    index: SqliteIndex,
}

impl Vault {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let index = SqliteIndex::open(dir.path()).unwrap();
        Self { _dir: dir, index }
    }

    fn note(&mut self, path: &str, source: &str) -> &mut Self {
        let parsed = MarkdownParser::new().parse(source);
        let f = VaultFile { path: path.into(), kind: FileKind::from_path(path), size: 1, mtime_ns: 1 };
        self.index.apply(&[IndexChange::Upsert { file: &f, note: Some((source, &parsed)) }]).unwrap();
        self
    }

    fn file(&mut self, path: &str) -> &mut Self {
        let f = VaultFile { path: path.into(), kind: FileKind::from_path(path), size: 1, mtime_ns: 1 };
        self.index.apply(&[IndexChange::Upsert { file: &f, note: None }]).unwrap();
        self
    }

    fn remove(&mut self, path: &str) -> &mut Self {
        self.index.apply(&[IndexChange::Remove { path }]).unwrap();
        self
    }

    /// Resolved target of each outgoing link of `path`.
    fn targets(&self, path: &str) -> Vec<Option<String>> {
        self.index.links_from(path).unwrap().into_iter().map(|l| l.resolved).collect()
    }
}

fn some(p: &str) -> Option<String> {
    Some(p.to_owned())
}

#[test]
fn wikilink_resolves_by_name_case_insensitively() {
    let mut v = Vault::new();
    v.note("Notes/B.md", "b").note("a.md", "[[B]] [[b]] [[B.md]]");
    assert_eq!(v.targets("a.md"), [some("Notes/B.md"), some("Notes/B.md"), some("Notes/B.md")]);
}

#[test]
fn path_in_link_picks_that_file() {
    let mut v = Vault::new();
    v.note("B.md", "root").note("X/B.md", "x").note("a.md", "[[X/B]] [[B]]");
    assert_eq!(v.targets("a.md"), [some("X/B.md"), some("B.md")]);
}

#[test]
fn ambiguous_name_takes_the_shortest_path() {
    let mut v = Vault::new();
    v.note("Y/Z/B.md", "deep").note("X/B.md", "x").note("Y/a.md", "[[B]]");
    assert_eq!(v.targets("Y/a.md"), [some("X/B.md")]);
}

#[test]
fn broken_link_resolves_once_the_target_appears_and_breaks_when_it_goes() {
    let mut v = Vault::new();
    v.note("a.md", "[[Later]]");
    assert_eq!(v.targets("a.md"), [None]);
    v.note("Folder/Later.md", "here now");
    assert_eq!(v.targets("a.md"), [some("Folder/Later.md")]);
    v.note("Later.md", "shorter path");
    assert_eq!(v.targets("a.md"), [some("Later.md")]);
    v.remove("Later.md");
    assert_eq!(v.targets("a.md"), [some("Folder/Later.md")]);
    v.remove("Folder/Later.md");
    assert_eq!(v.targets("a.md"), [None]);
}

#[test]
fn markdown_links_are_relative_to_the_source_folder_first() {
    let mut v = Vault::new();
    v.note("Folder/b.md", "near").note("b.md", "root").note("c.md", "c");
    v.note("Folder/a.md", "[x](b.md) [y](../c.md) [z](My%20Note.md)");
    v.note("Folder/My Note.md", "spaced");
    assert_eq!(v.targets("Folder/a.md"), [some("Folder/b.md"), some("c.md"), some("Folder/My Note.md")]);
}

#[test]
fn embeds_resolve_to_attachments_and_external_links_do_not_resolve() {
    let mut v = Vault::new();
    v.file("att/diagram.png").note("a.md", "![[diagram.png]] [site](https://x.y)");
    assert_eq!(v.targets("a.md"), [some("att/diagram.png"), None]);
}

#[test]
fn same_note_heading_link_resolves_to_itself() {
    let mut v = Vault::new();
    v.note("a.md", "# Top\n[[#Top]]");
    assert_eq!(v.targets("a.md"), [some("a.md")]);
}

#[test]
fn decomposed_file_names_match_composed_links() {
    let mut v = Vault::new();
    v.note("И\u{0306}од.md", "nfd name").note("a.md", "[[Йод]]");
    assert_eq!(v.targets("a.md"), [some("И\u{0306}од.md")]);
}

#[test]
fn backlinks_list_other_notes_with_their_links() {
    let mut v = Vault::new();
    v.note("b.md", "self [[b]]").note("c.md", "[[b|see b]] and [[b#Part]]").note("a.md", "[[B]]").note("d.md", "[[other]]");
    let back: Vec<(String, Option<String>)> =
        v.index.backlinks("b.md").unwrap().into_iter().map(|b| (b.source, b.link.display)).collect();
    assert_eq!(back, [("a.md".to_owned(), None), ("c.md".to_owned(), some("see b")), ("c.md".to_owned(), None)]);
}

#[test]
fn links_inside_one_batch_resolve_to_files_of_the_same_batch() {
    let mut v = Vault::new();
    let (a, b) = ("[[B]]", "b");
    let pa = MarkdownParser::new().parse(a);
    let pb = MarkdownParser::new().parse(b);
    let fa = VaultFile { path: "a.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
    let fb = VaultFile { path: "B.md".into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 };
    v.index
        .apply(&[IndexChange::Upsert { file: &fa, note: Some((a, &pa)) }, IndexChange::Upsert { file: &fb, note: Some((b, &pb)) }])
        .unwrap();
    assert_eq!(v.targets("a.md"), [some("B.md")]);
}
