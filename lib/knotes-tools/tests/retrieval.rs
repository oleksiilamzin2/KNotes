//! How well search finds what a question needs, without the model (ADR 0027): queries the chat's
//! model really wrote on the household vault (`tests/home`), each with the documents holding the
//! answer, run through the `search` tool. Recall@5: the share of queries with one of those in the
//! first five documents; MRR: the mean of 1 / its rank (top ten). Queries marked "meaning" share no
//! words with their documents (other words, a retelling, another language, dates told in words):
//! search by words is not expected to find them; they measure search by meaning. Nine were the
//! model's own; 56 more were written on 2026-11-15 from the vault's text, so that a change to
//! search by meaning can be told from noise (ADR 0028).
//!
//! Run three times: the tools' search by words alone, then with search by meaning (EmbeddingGemma,
//! ADR 0028; the model comes from `scripts/setup.sh`) merged in, then the note list's search by
//! meaning (`notes_by_meaning`, ADR 0015).
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
use knotes_tools::{Closeness, ToolIndex, VaultTools, notes_by_meaning};
use knotes_vectors::{MeaningIndex, MeaningModel, PIECE_CHARS, embed_notes};
use knotes_vault::FsVault;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

/// The household vault's scores by words (2026-11-14: 30 of 35; with titles, stems and links 33
/// of 35; with three wordings the model wrote next, 33 of 37; leaving question words out,
/// five-letter stems and all-but-one-word finds, 36 of 37): a change to search may raise them,
/// never lower them.
const WORDS_RECALL: f64 = 0.97;

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
    /// How many documents a query found, all told (the list shows them all).
    shown: usize,
}

impl Run {
    fn print(&self, name: &str) {
        println!("{name}:");
        println!("  {}", self.all.line("all"));
        println!("  {}", self.words.line("by words"));
        println!("  {}", self.meaning.line("by meaning"));
        println!("  documents found a query: {:.1}", self.shown as f64 / self.all.queries.max(1) as f64);
        println!("  Not in the first five:\n{}", self.missed.join("\n"));
    }
}

/// The paths the tools' search finds for `query`, best first.
fn tools_find<I: ToolIndex>(tools: &VaultTools<I, FsVault, PdfTexts>, query: &str) -> Vec<String> {
    let found = tools.call("search", &json!({"query": query, "limit": 10})).unwrap();
    found["results"].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap().to_string()).collect()
}

fn run(queries: &[Query], find: impl Fn(&str) -> Vec<String>) -> Run {
    let mut run = Run { all: Score::default(), words: Score::default(), meaning: Score::default(), missed: Vec::new(), shown: 0 };
    for query in queries {
        let mut paths: Vec<String> = Vec::new();
        for path in find(&query.text) {
            // Files macOS names keep their letters decomposed (й as и + ˘): compared composed.
            let path: String = path.nfc().collect();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        run.shown += paths.len();
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
    let words_only = tools();
    let by_words = run(&queries, |q| tools_find(&words_only, q));
    by_words.print("By words");

    // Each model search by meaning can run on (ADR 0029); KNOTES_RETRIEVAL_MODEL=<key> measures
    // one alone.
    let only = std::env::var("KNOTES_RETRIEVAL_MODEL").ok();
    for model in MeaningModel::ALL.into_iter().filter(|m| only.as_deref().is_none_or(|k| k == m.key())) {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(model.file());
        assert!(file.exists(), "no model at {}: run scripts/setup.sh", file.display());
        let embedder = Arc::new(LlamaEmbedder::load(&file).unwrap());
        // KNOTES_RETRIEVAL_PIECES=600 tries pieces of another size (0: blocks whole).
        let pieces = std::env::var("KNOTES_RETRIEVAL_PIECES").ok().and_then(|p| p.parse().ok()).map_or(PIECE_CHARS, |p: usize| if p == 0 { usize::MAX } else { p });
        let snapshot = embed_notes(&SqliteIndex::open_reader(index_dir.path()).unwrap(), &*embedder, pieces).unwrap();
        println!("\n{} ({} blocks embedded)", model.key(), snapshot.len());
        let list_meaning = MeaningIndex::new(embedder.clone(), snapshot.clone());
        let merged = tools().with_meaning(Box::new(MeaningIndex::new(embedder, snapshot)));
        let with_meaning = run(&queries, |q| tools_find(&merged, q));
        with_meaning.print("With meaning");
        let reader = SqliteIndex::open_reader(index_dir.path()).unwrap();
        // KNOTES_RETRIEVAL_CLOSENESS=0.80:0.03 tries other thresholds for the list (to tune them).
        let tried = std::env::var("KNOTES_RETRIEVAL_CLOSENESS").ok().and_then(|t| t.split_once(':').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))));
        let (near_enough, behind_nearest) = tried.unwrap_or(model.closeness());
        let closeness = Closeness { near_enough, behind_nearest };
        let list = run(&queries, |q| notes_by_meaning(&reader, &list_meaning, q, None, 40, closeness).into_iter().map(|h| h.path).collect());
        list.print("The list's search by meaning");
        if own.is_none() && tried.is_none() {
            let floors = floors(model);
            assert!(with_meaning.words.recall() >= WORDS_RECALL, "search by words got worse with {}: {}", model.key(), with_meaning.words.line("by words"));
            assert!(with_meaning.meaning.recall() >= floors.meaning, "search by meaning got worse with {}: {}", model.key(), with_meaning.meaning.line("by meaning"));
            assert!(list.words.recall() >= WORDS_RECALL && list.meaning.recall() >= floors.list, "the list's search got worse with {}: {} {}", model.key(), list.words.line("by words"), list.meaning.line("by meaning"));
            assert!(list.shown as f64 / list.all.queries as f64 <= floors.shown, "the list's search lists more noise with {}: {}", model.key(), list.shown);
        }
    }
    if own.is_none() {
        assert!(by_words.words.recall() >= WORDS_RECALL, "search by words got worse: {}", by_words.words.line("by words"));
    }
}

/// What a model's runs may not fall below: by meaning in the tools' search and in the list's,
/// and how many documents the list may show a query on average.
struct Floors {
    meaning: f64,
    list: f64,
    shown: f64,
}

/// Each model's scores (2026-11-15, 512 dimensions): a change to search may raise them, never
/// lower them.
fn floors(model: MeaningModel) -> Floors {
    match model {
        // 63 of 65 by meaning (MRR 0.81, 0.83 in pieces of 400), the list 63 of 65 (MRR 0.83,
        // 0.86 in pieces) showing 7.4 a query.
        MeaningModel::E5Large => Floors { meaning: 0.96, list: 0.96, shown: 7.6 },
        // 62 of 65 (MRR 0.78, 0.82 in pieces), the list 62 of 65 (MRR 0.81, 0.86 in pieces)
        // showing 6.3 a query.
        MeaningModel::EmbeddingGemma2 => Floors { meaning: 0.95, list: 0.95, shown: 6.8 },
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
