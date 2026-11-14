use std::path::Path;

use knotes_core::{DocumentPages, DocumentText};
use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_tools::{ToolError, VaultTools};
use knotes_vault::FsVault;
use serde_json::{Value, json};

/// A PDF's pages, as PDFKit would give them.
struct Pdfs;

impl DocumentText for Pdfs {
    fn document_text(&self, path: &str) -> Option<String> {
        path.ends_with(".pdf").then(|| "Budget overview\n\nThe radar budget grows next year.".to_owned())
    }
}

impl DocumentPages for Pdfs {
    fn page_count(&self, path: &str) -> Option<usize> {
        path.ends_with(".pdf").then_some(2)
    }

    fn page_text(&self, path: &str, page: usize) -> Option<String> {
        (path.ends_with(".pdf") && page < 2).then(|| ["Budget overview", "The radar budget grows next year."][page].to_owned())
    }

    fn outline(&self, path: &str) -> Vec<(u8, String, usize)> {
        if path.ends_with(".pdf") { vec![(1, "Overview".to_owned(), 0), (1, "Plans".to_owned(), 1)] } else { Vec::new() }
    }
}

struct Vault {
    _dirs: (tempfile::TempDir, tempfile::TempDir),
    tools: VaultTools<SqliteIndex, FsVault, Pdfs>,
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// A small vault, indexed, and its tools.
fn vault() -> Vault {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Projects/Radar.md", "# Radar\n\nThe radar project #work starts in May.\n\n## Team\n\nAnna and [[Boris]].\n\n## Budget\n\nSee ![[budget.pdf]].");
    write(dir.path(), "Boris.md", "# Boris\n\nWorks on [[Radar]]. #people");
    write(dir.path(), "Inbox/Idea.md", "A rough idea about gardens. #work");
    write(dir.path(), "budget.pdf", "%PDF-1.4");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs);
    Vault { _dirs: (dir, idx), tools }
}

fn call(vault: &Vault, name: &str, args: Value) -> Value {
    vault.tools.call(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn every_tool_is_described_with_a_json_schema() {
    let names: Vec<&str> = VaultTools::<SqliteIndex, FsVault, Pdfs>::specs().iter().map(|s| s.name).collect();
    assert_eq!(names, ["search", "find_by_title", "read_note", "read_pdf", "outline", "links", "list_notes", "list_tags"]);
    for spec in VaultTools::<SqliteIndex, FsVault, Pdfs>::specs() {
        assert_eq!(spec.parameters["type"], "object", "{}", spec.name);
        assert!(!spec.description.is_empty());
    }
}

#[test]
fn search_finds_fragments_of_notes_and_pdfs_with_their_place() {
    let v = vault();
    let found = call(&v, "search", json!({"query": "radar budget"}));
    let results = found["results"].as_array().unwrap();
    let pdf = results.iter().find(|r| r["path"] == "budget.pdf").unwrap_or_else(|| panic!("{found}"));
    assert_eq!(pdf["kind"], "pdf");
    assert!(pdf["text"].as_str().unwrap().contains("radar budget"), "{pdf}");
    let found = call(&v, "search", json!({"query": "Anna"}));
    let note = &found["results"][0];
    assert_eq!((note["path"].as_str(), note["title"].as_str(), note["heading"].as_str()), (Some("Projects/Radar.md"), Some("Radar"), Some("Team")), "{found}");
    assert!(note["block"].is_u64());
}

#[test]
fn notes_are_found_by_title() {
    let found = call(&vault(), "find_by_title", json!({"query": "bor"}));
    assert_eq!(found["results"][0]["path"], "Boris.md", "{found}");
}

#[test]
fn a_note_is_read_by_blocks_with_their_headings() {
    let v = vault();
    let read = call(&v, "read_note", json!({"path": "Projects/Radar.md"}));
    assert_eq!(read["title"], "Radar");
    let blocks = read["blocks"].as_array().unwrap();
    assert!(blocks.iter().any(|b| b["heading"] == "Team" && b["text"].as_str().unwrap().contains("Anna")), "{read}");
    let total = read["total_blocks"].as_u64().unwrap();
    let tail = call(&v, "read_note", json!({"path": "Projects/Radar.md", "from_block": total - 1}));
    assert_eq!(tail["blocks"].as_array().unwrap().len(), 1);
}

#[test]
fn a_pdf_is_read_by_pages_counted_from_one() {
    let read = call(&vault(), "read_pdf", json!({"path": "budget.pdf", "from_page": 2}));
    assert_eq!((read["page_count"].as_u64(), read["pages"][0]["page"].as_u64()), (Some(2), Some(2)));
    assert!(read["pages"][0]["text"].as_str().unwrap().contains("grows"));
}

#[test]
fn outlines_links_notes_and_tags_are_listed() {
    let v = vault();
    let outline = call(&v, "outline", json!({"path": "Projects/Radar.md"}));
    let titles: Vec<&str> = outline["headings"].as_array().unwrap().iter().map(|h| h["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Radar", "Team", "Budget"]);
    assert_eq!(call(&v, "outline", json!({"path": "budget.pdf"}))["headings"][1]["page"], 2);
    let links = call(&v, "links", json!({"path": "Projects/Radar.md"}));
    assert_eq!(links["backlinks"][0]["path"], "Boris.md", "{links}");
    let out: Vec<Option<&str>> = links["outgoing"].as_array().unwrap().iter().map(|o| o["path"].as_str()).collect();
    assert_eq!(out, [Some("Boris.md"), Some("budget.pdf")]);
    let work = call(&v, "list_notes", json!({"tag": "work"}));
    let paths: Vec<&str> = work["notes"].as_array().unwrap().iter().map(|n| n["path"].as_str().unwrap()).collect();
    assert_eq!(paths.len(), 2, "{work}");
    assert!(paths.contains(&"Inbox/Idea.md") && paths.contains(&"Projects/Radar.md"));
    let inbox = call(&v, "list_notes", json!({"folder": "Inbox"}));
    assert_eq!(inbox["notes"][0]["path"], "Inbox/Idea.md");
    let tags = call(&v, "list_tags", json!({}));
    assert_eq!(tags["tags"][0], json!({"tag": "work", "count": 2}), "{tags}");
}

#[test]
fn wrong_calls_say_what_is_wrong() {
    let v = vault();
    assert!(matches!(v.tools.call("read_note", &json!({"path": "Nope.md"})), Err(ToolError::NotFound(_))));
    assert!(matches!(v.tools.call("read_note", &json!({})), Err(ToolError::BadArguments(_))));
    assert!(matches!(v.tools.call("delete_everything", &json!({})), Err(ToolError::Unknown(_))));
    assert!(matches!(v.tools.call("read_note", &json!({"path": "budget.pdf"})), Err(ToolError::BadArguments(_))), "a PDF is read with read_pdf");
}
