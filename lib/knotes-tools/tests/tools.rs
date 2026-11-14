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
    assert_eq!(names, ["search", "find_by_title", "read_note", "read_pdf", "outline", "links", "related", "list_notes", "list_tasks", "list_tags"]);
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

/// A vault of `n` notes all naming apples, each changed a day after the one before
/// (note 0 on 2026-01-01).
fn orchard(n: usize) -> Vault {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let start = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_767_225_600); // 2026-01-01T00:00Z
    for i in 0..n {
        let path = format!("Orchard/Apple {i:02}.md");
        write(dir.path(), &path, &format!("# Apple {i:02}\n\nThe apple tree number {i}."));
        let file = std::fs::File::options().write(true).open(dir.path().join(&path)).unwrap();
        file.set_modified(start + std::time::Duration::from_secs(86_400 * i as u64 + 3_600)).unwrap();
    }
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs);
    Vault { _dirs: (dir, idx), tools }
}

fn paths(result: &Value, key: &str) -> Vec<String> {
    result[key].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap().to_string()).collect()
}

#[test]
fn search_title_search_and_lists_go_on_a_page_at_a_time() {
    let v = orchard(25);
    let mut seen = Vec::new();
    let mut offset = Some(0);
    let mut pages = 0;
    while let Some(at) = offset {
        let page = call(&v, "search", json!({"query": "apple tree", "limit": 10, "offset": at}));
        seen.extend(paths(&page, "results"));
        offset = page["next_offset"].as_u64();
        pages += 1;
    }
    assert_eq!(pages, 3);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 25, "every note once over the pages");

    let first = call(&v, "find_by_title", json!({"query": "apple", "limit": 20}));
    assert_eq!((paths(&first, "results").len(), first["next_offset"].as_u64()), (20, Some(20)));
    let rest = call(&v, "find_by_title", json!({"query": "apple", "limit": 20, "offset": 20}));
    assert_eq!((paths(&rest, "results").len(), rest["next_offset"].as_u64()), (5, None));

    let listed = call(&v, "list_notes", json!({"folder": "Orchard", "limit": 10, "offset": 20}));
    assert_eq!(listed["total"], 25);
    assert_eq!(paths(&listed, "notes"), ["Orchard/Apple 04.md", "Orchard/Apple 03.md", "Orchard/Apple 02.md", "Orchard/Apple 01.md", "Orchard/Apple 00.md"], "newest first");
    assert!(listed["next_offset"].is_null());
}

#[test]
fn notes_are_listed_by_the_days_they_changed_with_their_dates() {
    let v = orchard(25);
    let january = call(&v, "list_notes", json!({"changed_after": "2026-01-10", "changed_before": "2026-01-12"}));
    assert_eq!(paths(&january, "notes"), ["Orchard/Apple 11.md", "Orchard/Apple 10.md", "Orchard/Apple 09.md"], "both days included");
    assert_eq!(january["notes"][0]["modified"], "2026-01-12T01:00Z");
    assert_eq!(january["total"], 3);
    let wrong = v.tools.call("list_notes", &json!({"changed_after": "last week"}));
    assert!(matches!(wrong, Err(knotes_tools::ToolError::BadArguments(_))), "{wrong:?}");
}

#[test]
fn a_long_read_says_where_to_go_on() {
    let v = vault();
    let first = call(&v, "read_note", json!({"path": "Projects/Radar.md", "max_blocks": 2}));
    assert_eq!(first["next_block"], 2);
    let last = call(&v, "read_note", json!({"path": "Projects/Radar.md", "from_block": 2}));
    assert!(last["next_block"].is_null(), "{last}");
    let pdf = call(&v, "read_pdf", json!({"path": "budget.pdf", "pages": 1}));
    assert_eq!(pdf["next_page"], 2, "{pdf}");
}

#[test]
fn words_spread_over_a_note_still_find_it_after_the_fragments_holding_them_all() {
    let v = vault();
    let spread = call(&v, "search", json!({"query": "radar Anna"}));
    let results = spread["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{spread}");
    assert_eq!((results[0]["path"].as_str(), results[0]["match"].as_str()), (Some("Projects/Radar.md"), Some("note")), "{spread}");

    let together = call(&v, "search", json!({"query": "radar project"}));
    assert_eq!(together["results"][0]["match"], "block", "{together}");
    let paths: Vec<&str> = together["results"].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap()).collect();
    assert_eq!(paths.iter().filter(|p| **p == "Projects/Radar.md").count(), 1, "a note found by a fragment is not listed again: {paths:?}");
}

#[test]
fn the_vault_is_described_in_a_few_words_for_the_model() {
    let v = vault();
    let overview = v.tools.overview();
    assert!(overview.starts_with("The vault: 3 notes and 1 PDF."), "{overview}");
    assert!(overview.contains("Top-level folders (files in each): Inbox (1), Projects (1); 2 files at the top."), "{overview}");
    assert!(overview.contains("Most used tags: work (2), people (1)."), "{overview}");
    assert!(overview.contains("titles are mostly in Latin script"), "{overview}");
    let orchard = orchard(3).tools.overview();
    assert!(orchard.contains("Orchard (3)") && !orchard.contains("at the top"), "{orchard}");
}

/// A vault where words hide: in an alias, a title, another form of the word.
fn hidden() -> Vault {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Car.md", "---\naliases: [Машина]\n---\n# Car\n\nSee [[Log]].");
    write(dir.path(), "Log.md", "# Log\n\nOil changed in May.");
    write(dir.path(), "Manuals/Boiler guide.md", "Scale on the heater: clean it.");
    write(dir.path(), "Codes.md", "Код помилок E9: накип.");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs);
    Vault { _dirs: (dir, idx), tools }
}

#[test]
fn search_finds_words_hidden_in_titles_aliases_links_and_other_forms() {
    let v = hidden();
    let first = |query: &str| call(&v, "search", json!({"query": query}))["results"][0].clone();
    let linked = first("oil машин");
    assert_eq!((linked["path"].as_str(), linked["match"].as_str(), linked["via"].as_str()), (Some("Log.md"), Some("linked"), Some("Car.md")), "a word naming a note leads to the notes it links: {linked}");
    let titled = first("boiler guide");
    assert_eq!((titled["path"].as_str(), titled["match"].as_str()), (Some("Manuals/Boiler guide.md"), Some("title")), "{titled}");
    let stem = first("помилка E9");
    assert_eq!((stem["path"].as_str(), stem["match"].as_str()), (Some("Codes.md"), Some("stem")), "{stem}");
    assert_eq!(first("oil changed")["match"], "block", "the words together still come first");
}

#[test]
fn question_words_are_left_out_and_short_words_cut_to_their_stems() {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Марко.md", "# Марко\n\n## Гуртки\n\nФутбол у вівторок.");
    write(dir.path(), "Log.md", "Oil changed in May.");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let v = Vault { tools: VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs), _dirs: (dir, idx) };
    let first = |query: &str| call(&v, "search", json!({"query": query}))["results"][0].clone();
    assert_eq!(first("when was oil changed")["match"], "block", "\"when\" and \"was\" are left out");
    let marko = first("Марка гурток коли");
    assert_eq!((marko["path"].as_str(), marko["match"].as_str()), (Some("Марко.md"), Some("stem")), "{marko}");
    assert!(call(&v, "search", json!({"query": "коли"}))["results"].as_array().unwrap().is_empty(), "a query of question words alone is searched as it is");
}

#[test]
fn a_query_missing_one_word_still_finds_the_rest_and_names_the_word() {
    let v = hidden();
    let partial = call(&v, "search", json!({"query": "oil changed yesterday"}))["results"][0].clone();
    assert_eq!((partial["path"].as_str(), partial["match"].as_str(), partial["missing"].as_str()), (Some("Log.md"), Some("partial"), Some("yesterday")), "{partial}");
}

/// A task as listed: its line, words and section.
type Task = (u64, String, Option<String>);

#[test]
fn tasks_are_listed_by_note_with_their_line_and_section() {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Inbox/Buy.md", "# Buy\n\n- [ ] milk\n- [x] bread\n\n## Later\n\n- [ ] eggs");
    write(dir.path(), "Plan.md", "Plan\n\n* [ ] call Bob\n1. [ ] book tickets");
    write(dir.path(), "Done.md", "- [x] all done");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let v = Vault { tools: VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs), _dirs: (dir, idx) };
    let open = call(&v, "list_tasks", json!({}));
    let mut notes: Vec<(String, Vec<Task>)> = open["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["path"].as_str().unwrap().to_string(), n["tasks"].as_array().unwrap().iter().map(|t| (t["line"].as_u64().unwrap(), t["text"].as_str().unwrap().to_string(), t["heading"].as_str().map(str::to_string))).collect()))
        .collect();
    notes.sort();
    assert_eq!(notes, vec![
        ("Inbox/Buy.md".to_string(), vec![(3, "milk".to_string(), Some("Buy".to_string())), (8, "eggs".to_string(), Some("Later".to_string()))]),
        ("Plan.md".to_string(), vec![(3, "call Bob".to_string(), None), (4, "book tickets".to_string(), None)]),
    ]);
    assert_eq!(open["total"], 2);
    let done = call(&v, "list_tasks", json!({"state": "done"}));
    assert_eq!(done["total"], 2, "{done}");
    let inbox = call(&v, "list_tasks", json!({"folder": "Inbox"}));
    assert_eq!(inbox["notes"].as_array().unwrap().len(), 1);
}

#[test]
fn a_note_comes_with_the_notes_linked_to_and_from_it() {
    let v = vault();
    let related = call(&v, "related", json!({"path": "Projects/Radar.md"}));
    assert!(related["text"].as_str().unwrap().contains("The radar project"), "{related}");
    let to: Vec<&str> = related["links_to"].as_array().unwrap().iter().map(|n| n["path"].as_str().unwrap()).collect();
    assert_eq!(to, ["Boris.md", "budget.pdf"]);
    assert!(related["links_to"][0]["start"].as_str().unwrap().starts_with("Works on [[Radar]]"), "{related}");
    let from = &related["linked_from"][0];
    assert_eq!((from["path"].as_str(), from["line"].as_str()), (Some("Boris.md"), Some("Works on [[Radar]]. #people")), "{related}");
}

/// A vault with what the user keeps from the AI: a folder (`"ai": false`, a subfolder taking it
/// back with `"ai": true`) and a note tagged `#no-ai`.
fn private() -> Vault {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Private/.knotes.json", r#"{"ai": false}"#);
    write(dir.path(), "Private/Secret.md", "# Secret\n\nThe secret code is 1234. #money\n\n- [ ] change the code");
    write(dir.path(), "Private/Shared/.knotes.json", r#"{"ai": true}"#);
    write(dir.path(), "Private/Shared/Plan.md", "# Plan\n\nThe secret plan for May. #money");
    write(dir.path(), "Diary.md", "# Diary\n\nMy secret diary. #no-ai\n\n- [ ] write more");
    write(dir.path(), "Open.md", "# Open\n\nA secret? See [[Secret]], [[Diary]] and [[Plan]].");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs);
    Vault { _dirs: (dir, idx), tools }
}

#[test]
fn what_the_user_keeps_from_the_ai_no_tool_shows() {
    let v = private();
    let mut found = paths(&call(&v, "search", json!({"query": "secret"})), "results");
    found.sort();
    assert_eq!(found, ["Open.md", "Private/Shared/Plan.md"], "a folder kept out, its shared subfolder in, a #no-ai note out");
    assert!(paths(&call(&v, "find_by_title", json!({"query": "secret"})), "results").is_empty());
    for (tool, args) in [("read_note", json!({"path": "Private/Secret.md"})), ("outline", json!({"path": "Diary.md"})), ("links", json!({"path": "Diary.md"})), ("related", json!({"path": "Private/Secret.md"}))] {
        let refused = v.tools.call(tool, &args);
        assert!(matches!(refused, Err(knotes_tools::ToolError::Excluded(_))), "{tool}: {refused:?}");
        assert!(refused.unwrap_err().to_string().contains("not shared with the AI"));
    }
    let mut listed = paths(&call(&v, "list_notes", json!({})), "notes");
    listed.sort();
    assert_eq!(listed, ["Open.md", "Private/Shared/Plan.md"]);
    assert!(call(&v, "list_tasks", json!({}))["notes"].as_array().unwrap().is_empty(), "the kept notes' tasks too");
    let links = call(&v, "links", json!({"path": "Open.md"}));
    let outgoing: Vec<&str> = links["outgoing"].as_array().unwrap().iter().filter_map(|o| o["path"].as_str()).collect();
    assert_eq!(outgoing, ["Private/Shared/Plan.md"], "links to kept notes are left out");
    let related: Vec<String> = paths(&call(&v, "related", json!({"path": "Open.md"})), "links_to");
    assert_eq!(related, ["Private/Shared/Plan.md"]);
    let tags = call(&v, "list_tags", json!({}));
    assert_eq!(tags["tags"], json!([{"tag": "money", "count": 1}]), "counted without the kept notes, the marking tag not shown: {tags}");
    assert!(v.tools.overview().starts_with("The vault: 2 notes"), "{}", v.tools.overview());
}

#[test]
fn found_and_read_notes_say_when_they_last_changed() {
    let v = orchard(5);
    let day = |path: &str| format!("2026-01-{:02}T01:00Z", path["Orchard/Apple ".len()..][..2].parse::<u32>().unwrap() + 1);
    let found = call(&v, "search", json!({"query": "apple tree"}));
    for result in found["results"].as_array().unwrap() {
        let path = result["path"].as_str().unwrap();
        assert_eq!(result["modified"], day(path), "{path}");
    }
    let read = call(&v, "read_note", json!({"path": "Orchard/Apple 03.md"}));
    assert_eq!(read["modified"], "2026-01-04T01:00Z");
}

/// Search by meaning as a list given beforehand, or failing.
struct Near(Option<Vec<(&'static str, u32)>>);

impl knotes_core::MeaningSearch for Near {
    fn nearest(&self, _query: &str, limit: usize) -> Result<Vec<knotes_core::BlockHit>, knotes_core::BoxError> {
        let hits = self.0.as_ref().ok_or("the helper is gone")?;
        Ok(hits.iter().take(limit).map(|(path, ord)| knotes_core::BlockHit { path: path.to_string(), ord: *ord, span: knotes_core::Span { start: 0, end: 0 }, heading: None, score: 0.0 }).collect())
    }
}

fn with_meaning(near: Near) -> Vault {
    let (dir, idx) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(dir.path(), "Fruit/Apple.md", "Apple trees bloom in May.");
    write(dir.path(), "Fruit/Pear.md", "Pears ripen in September.");
    write(dir.path(), "Diary.md", "Picked fruit for the jam. #no-ai");
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    Indexer::with_documents(FsVault::new(dir.path()), MarkdownParser::new(), Pdfs).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(idx.path()).unwrap(), FsVault::new(dir.path()), Pdfs).with_meaning(Box::new(near));
    Vault { _dirs: (dir, idx), tools }
}

#[test]
fn finds_by_meaning_join_those_by_words_which_keep_the_lead() {
    let v = with_meaning(Near(Some(vec![("Fruit/Pear.md", 0), ("Diary.md", 0), ("Fruit/Apple.md", 0)])));
    let found = call(&v, "search", json!({"query": "apple"}));
    let results: Vec<(&str, &str)> = found["results"].as_array().unwrap().iter().map(|r| (r["path"].as_str().unwrap(), r["match"].as_str().unwrap())).collect();
    assert_eq!(results, [("Fruit/Apple.md", "block"), ("Fruit/Pear.md", "meaning")], "the find by words first, once; the note kept from the AI never");
}

#[test]
fn search_goes_on_by_words_when_search_by_meaning_fails() {
    let v = with_meaning(Near(None));
    assert_eq!(paths(&call(&v, "search", json!({"query": "apple"})), "results"), ["Fruit/Apple.md"]);
}
