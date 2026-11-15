//! How well search finds what a question needs, without the model (ADR 0027): queries the chat's
//! model really wrote on the household vault (`tests/home`), each with the documents holding the
//! answer, run through the `search` tool. Recall@5: the share of queries with one of those in the
//! first five documents; MRR: the mean of 1 / its rank (top ten). Queries marked "meaning" share no
//! words with their documents (other words, a retelling, another language, dates told in words):
//! search by words is not expected to find them; they measure search by meaning. Nine were the
//! model's own; 56 more were written on 2026-11-15 from the vault's text, so that a change to
//! search by meaning can be told from noise (ADR 0028).
//!
//! Run twice: by words alone, then with search by meaning (EmbeddingGemma, ADR 0028; the model
//! comes from `scripts/setup.sh`) merged in.
//!
//! `cargo test -p knotes-tools --test retrieval -- --nocapture` prints the report. Another vault:
//! `KNOTES_RETRIEVAL_VAULT=… KNOTES_RETRIEVAL_QUERIES=….json` (then nothing is asserted).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_pdf::PdfTexts;
use knotes_llama::LlamaEmbedder;
use knotes_tools::{ToolIndex, VaultTools};
use knotes_vectors::{MeaningIndex, embed_notes};
use knotes_vault::FsVault;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

/// The household vault's scores by words (2026-11-14: 30 of 35; with titles, stems and links 33
/// of 35; with three wordings the model wrote next, 33 of 37; leaving question words out,
/// five-letter stems and all-but-one-word finds, 36 of 37): a change to search may raise them,
/// never lower them.
const WORDS_RECALL: f64 = 0.97;
/// With search by meaning merged in (EmbeddingGemma Q8_0 over 100 blocks): 2026-11-14, by words
/// 37 of 37, by meaning 6 of 9; 2026-11-15, on 65 queries by meaning (5 found by words alone),
/// 53 of 65 (MRR 0.71), all 90 of 102.
const MEANING_RECALL: f64 = 0.81;
const ALL_WITH_MEANING: f64 = 0.88;

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

/// What a run of the queries found.
struct Run {
    all: Score,
    words: Score,
    meaning: Score,
    missed: Vec<String>,
}

impl Run {
    fn print(&self, name: &str) {
        println!("{name}:");
        println!("  {}", self.all.line("all"));
        println!("  {}", self.words.line("by words"));
        println!("  {}", self.meaning.line("by meaning"));
        println!("  Not in the first five:\n{}", self.missed.join("\n"));
    }
}

fn run<I: ToolIndex>(tools: &VaultTools<I, FsVault, PdfTexts>, queries: &[Query]) -> Run {
    let mut run = Run { all: Score::default(), words: Score::default(), meaning: Score::default(), missed: Vec::new() };
    for query in queries {
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
        run.all.add(rank);
        if query.by_meaning { run.meaning.add(rank) } else { run.words.add(rank) }
        if !rank.is_some_and(|r| r <= 5) {
            let kind = if query.by_meaning { "meaning" } else { "words" };
            let got = if paths.is_empty() { "nothing".to_string() } else { paths.iter().take(3).cloned().collect::<Vec<_>>().join(", ") };
            run.missed.push(format!("    [{kind}] \"{}\" → {got}", query.text));
        }
    }
    run
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
    let tools = || VaultTools::new(SqliteIndex::open_reader(index_dir.path()).unwrap(), FsVault::new(&vault), PdfTexts::new(&vault));

    println!("Retrieval on {} ({} queries)", vault.display(), queries.len());
    let by_words = run(&tools(), &queries);
    by_words.print("By words");

    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/embeddinggemma-300M-Q8_0.gguf");
    assert!(model.exists(), "no model at {}: run scripts/setup.sh", model.display());
    let embedder = Arc::new(LlamaEmbedder::load(&model).unwrap());
    let snapshot = embed_notes(&SqliteIndex::open_reader(index_dir.path()).unwrap(), &*embedder).unwrap();
    println!("({} blocks embedded)", snapshot.len());
    let with_meaning = run(&tools().with_meaning(Box::new(MeaningIndex::new(embedder, snapshot))), &queries);
    with_meaning.print("With meaning");

    if own.is_none() {
        for (name, run) in [("by words", &by_words), ("with meaning", &with_meaning)] {
            assert!(run.words.recall() >= WORDS_RECALL, "search by words got worse {name}: {}", run.words.line("by words"));
        }
        assert!(with_meaning.meaning.recall() >= MEANING_RECALL, "search by meaning got worse: {}", with_meaning.meaning.line("by meaning"));
        assert!(with_meaning.all.recall() >= ALL_WITH_MEANING, "search with meaning got worse: {}", with_meaning.all.line("all"));
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
