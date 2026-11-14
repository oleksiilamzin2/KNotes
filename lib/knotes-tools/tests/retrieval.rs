//! How well search finds what a question needs, without the model (ADR 0027): queries the chat's
//! model really wrote on the household vault (`tests/home`), each with the documents holding the
//! answer, run through the `search` tool. Recall@5: the share of queries with one of those in the
//! first five documents; MRR: the mean of 1 / its rank (top ten). Queries marked "meaning" share no
//! words with their documents: search by words is not expected to find them; they measure what
//! search by meaning would add.
//!
//! `cargo test -p knotes-tools --test retrieval -- --nocapture` prints the report. Another vault:
//! `KNOTES_RETRIEVAL_VAULT=… KNOTES_RETRIEVAL_QUERIES=….json` (then nothing is asserted).

use std::path::{Path, PathBuf};

use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_pdf::PdfTexts;
use knotes_tools::VaultTools;
use knotes_vault::FsVault;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

/// The household vault's scores (2026-11-14: by words 30 of 35, all 30 of 43): a change to
/// search may raise them, never lower them.
const WORDS_RECALL: f64 = 0.85;
const ALL_RECALL: f64 = 0.69;

struct Query {
    text: String,
    expected: Vec<String>,
    by_meaning: bool,
}

#[derive(Default)]
struct Score {
    queries: usize,
    hits: usize,
    reciprocal: f64,
}

impl Score {
    fn add(&mut self, rank: Option<usize>) {
        self.queries += 1;
        if rank.is_some_and(|r| r <= 5) {
            self.hits += 1;
        }
        self.reciprocal += rank.map_or(0.0, |r| 1.0 / r as f64);
    }

    fn recall(&self) -> f64 {
        if self.queries == 0 { 0.0 } else { self.hits as f64 / self.queries as f64 }
    }

    fn line(&self, name: &str) -> String {
        let mrr = if self.queries == 0 { 0.0 } else { self.reciprocal / self.queries as f64 };
        format!("{name} ({}): Recall@5 {:.2}, MRR {mrr:.2}", self.queries, self.recall())
    }
}

#[test]
fn search_finds_what_the_model_looked_for() {
    let home = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/home");
    let own = std::env::var_os("KNOTES_RETRIEVAL_VAULT").map(PathBuf::from);
    let vault = own.clone().unwrap_or_else(|| home.join("vault"));
    let queries_file = std::env::var_os("KNOTES_RETRIEVAL_QUERIES").map(PathBuf::from).unwrap_or_else(|| home.join("queries.json"));
    let queries = read_queries(&queries_file);

    let index_dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(index_dir.path()).unwrap();
    Indexer::with_documents(FsVault::new(&vault), MarkdownParser::new(), PdfTexts::new(&vault)).sync(&mut index).unwrap();
    let tools = VaultTools::new(SqliteIndex::open_reader(index_dir.path()).unwrap(), FsVault::new(&vault), PdfTexts::new(&vault));

    let (mut words, mut meaning, mut all) = (Score::default(), Score::default(), Score::default());
    let mut missed = Vec::new();
    for query in &queries {
        let found = tools.call("search", &json!({"query": query.text, "limit": 10})).unwrap();
        let mut paths: Vec<String> = Vec::new();
        for result in found["results"].as_array().unwrap() {
            // Files macOS names keep their letters decomposed (й as и + ˘): compared composed.
            let path: String = result["path"].as_str().unwrap().nfc().collect();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        let rank = paths.iter().position(|p| query.expected.contains(p)).map(|i| i + 1);
        all.add(rank);
        if query.by_meaning { meaning.add(rank) } else { words.add(rank) }
        if !rank.is_some_and(|r| r <= 5) {
            let kind = if query.by_meaning { "meaning" } else { "words" };
            let got = if paths.is_empty() { "nothing".to_string() } else { paths.iter().take(3).cloned().collect::<Vec<_>>().join(", ") };
            missed.push(format!("  [{kind}] \"{}\" → {got}", query.text));
        }
    }
    println!("Retrieval on {} ({} queries)", vault.display(), queries.len());
    println!("{}", all.line("all"));
    println!("{}", words.line("by words"));
    println!("{}", meaning.line("by meaning"));
    println!("Not in the first five:\n{}", missed.join("\n"));
    if own.is_none() {
        assert!(words.recall() >= WORDS_RECALL, "search by words got worse: {}", words.line("by words"));
        assert!(all.recall() >= ALL_RECALL, "search got worse: {}", all.line("all"));
    }
}

fn read_queries(path: &Path) -> Vec<Query> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|q| Query {
            text: q["query"].as_str().unwrap().to_string(),
            expected: q["expected"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().nfc().collect()).collect(),
            by_meaning: q["kind"] == "meaning",
        })
        .collect()
}
