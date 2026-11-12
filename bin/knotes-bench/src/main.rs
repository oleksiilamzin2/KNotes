//! `knotes-bench <vault> [--m1-factor F]`: indexes the vault into a temporary index (the vault
//! is only read) and prints a Markdown report against the budgets of ADR 0006.

mod report;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use knotes_core::{
    CaseMode, ContentScan, ContentSearch, CorpusSource, FileKind, IndexChange, IndexWriter, LinkGraph, NoteCatalog,
    NoteParser, NoteText, ScanQuery, TextCorpus, TitleSearch, VaultFile, VaultScanner,
};
use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_search::{RegexScanner, ScopedCorpus, TitleIndex};
use knotes_vault::FsVault;
use report::{Row, quantile, render};

const MS: fn(u64) -> Duration = Duration::from_millis;
/// Repetitions of each query.
const RUNS: usize = 5;

/// The app's allocator, so measurements match it (ADR 0014).
#[global_allocator]
static ALLOCATOR: knotes_alloc::DirectLarge = knotes_alloc::DirectLarge;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut vault, mut factor) = (None, 2.0);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--m1-factor" => factor = args.next().and_then(|v| v.parse().ok()).unwrap_or(factor),
            path => vault = Some(PathBuf::from(path)),
        }
    }
    let Some(vault) = vault else {
        eprintln!("usage: knotes-bench <vault> [--m1-factor F]");
        return ExitCode::FAILURE;
    };
    match run(vault, factor) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("knotes-bench: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(vault: PathBuf, factor: f64) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let mut index = SqliteIndex::open(dir.path())?;
    let fs_vault = FsVault::new(&vault);
    let mut rows = Vec::new();

    let t = Instant::now();
    let files = fs_vault.scan()?;
    rows.push(Row { operation: format!("Scan vault ({} files)", files.len()), measured: t.elapsed(), budget: None });

    let indexer = Indexer::new(FsVault::new(&vault), MarkdownParser::new());
    let t = Instant::now();
    indexer.sync(&mut index)?;
    rows.push(Row { operation: "Cold full index".into(), measured: t.elapsed(), budget: Some(Duration::from_secs(20)) });
    // Idle-time segment merging after the cold build, run to completion (the app spreads it
    // over 200 ms slices); later queries see the steady state.
    let t = Instant::now();
    let mut steps = 0;
    loop {
        let report = index.maintenance()?;
        steps += report.merge_steps;
        if report.merge_steps == 0 {
            break;
        }
    }
    rows.push(Row { operation: format!("Idle maintenance after cold index ({steps} merges)"), measured: t.elapsed(), budget: None });
    let t = Instant::now();
    indexer.sync(&mut index)?;
    rows.push(Row { operation: "Warm start: detect changes".into(), measured: t.elapsed(), budget: Some(Duration::from_secs(1)) });

    let catalog = index.catalog()?;
    let t = Instant::now();
    let titles = TitleIndex::new(catalog.iter().map(|e| (e.path.clone(), e.aliases.clone())));
    rows.push(Row { operation: "Build title index".into(), measured: t.elapsed(), budget: None });

    let corpus = index.corpus()?;
    let words = sample_words(&*corpus);
    let prefixes: Vec<String> = catalog.iter().step_by((catalog.len() / 30).max(1)).filter_map(|e| title_prefix(&e.path)).collect();

    rows.push(measure("Title search, per keystroke", MS(5), &prefixes, |q| titles.search_titles(q, 50).len()));
    rows.push(measure("Content search (FTS), 1 word", MS(30), &words, |q| index.search_blocks(q, None, 50).map_or(0, |h| h.len())));
    let pairs: Vec<String> = words.chunks(2).filter(|c| c.len() == 2).map(|c| c.join(" ")).collect();
    rows.push(measure("Content search (FTS), 2 words", MS(30), &pairs, |q| index.search_blocks(q, None, 50).map_or(0, |h| h.len())));

    let scope: Vec<String> = corpus_paths(&*corpus).into_iter().step_by(5).collect();
    rows.push(measure("Scoped content search (20% of notes)", MS(30), &words, |q| {
        index.search_blocks(q, Some(&scope), 50).map_or(0, |h| h.len())
    }));
    rows.push(measure("Regex scan, literal", MS(300), &words, |q| {
        RegexScanner.scan(&*corpus, &ScanQuery { pattern: q, regex: false, case: CaseMode::Smart }, 1000).map_or(0, |h| h.len())
    }));
    let rare = ["zq9xw7".to_owned()];
    rows.push(measure("Regex scan, no match (full pass)", MS(300), &rare, |q| {
        RegexScanner.scan(&*corpus, &ScanQuery { pattern: q, regex: false, case: CaseMode::Smart }, 1000).map_or(0, |h| h.len())
    }));
    rows.push(measure("Scoped regex scan (20% of notes)", MS(300), &words, |q| {
        let scoped = ScopedCorpus::new(&*corpus, &scope);
        RegexScanner.scan(&scoped, &ScanQuery { pattern: q, regex: false, case: CaseMode::Smart }, 1000).map_or(0, |h| h.len())
    }));

    let notes: Vec<String> = corpus_paths(&*corpus).into_iter().step_by((corpus.len() / 30).max(1)).collect();
    rows.push(measure("Backlinks of a note", MS(16), &notes, |p| index.backlinks(p).map_or(0, |b| b.len())));
    rows.push(reindex(&mut index, &notes)?);

    let header = format!(
        "Vault: `{}`, {} files, {} notes. M1 estimate = measured × {factor}. Query times are p95 of {RUNS} runs over sampled queries.\n\n",
        vault.display(),
        files.len(),
        corpus.len()
    );
    Ok(header + &render(&rows, factor))
}

/// p95 over every query repeated `RUNS` times.
fn measure(operation: &str, budget: Duration, queries: &[String], mut f: impl FnMut(&str) -> usize) -> Row {
    let mut samples = Vec::with_capacity(queries.len() * RUNS);
    for q in queries {
        for _ in 0..RUNS {
            let t = Instant::now();
            std::hint::black_box(f(q));
            samples.push(t.elapsed());
        }
    }
    let measured = if samples.is_empty() { Duration::ZERO } else { quantile(&mut samples, 0.95) };
    Row { operation: format!("{operation} ({} queries)", queries.len()), measured, budget: Some(budget) }
}

/// Re-indexes sampled notes one at a time, as after an edit.
fn reindex(index: &mut SqliteIndex, notes: &[String]) -> Result<Row, Box<dyn std::error::Error + Send + Sync>> {
    let parser = MarkdownParser::new();
    let mut samples = Vec::new();
    for path in notes {
        let Some(mut text) = index.note_text(path)? else { continue };
        text.push_str("\nedited");
        let parsed = parser.parse(&text);
        let file = VaultFile { path: path.clone(), kind: FileKind::Markdown, size: text.len() as u64, mtime_ns: 1 };
        let t = Instant::now();
        index.apply(&[IndexChange::Upsert { file: &file, note: Some((&text, &parsed)) }])?;
        samples.push(t.elapsed());
    }
    let measured = if samples.is_empty() { Duration::ZERO } else { quantile(&mut samples, 0.95) };
    Ok(Row { operation: format!("Reindex one edited note ({} notes)", samples.len()), measured, budget: Some(MS(10)) })
}

fn corpus_paths(corpus: &impl TextCorpus) -> Vec<String> {
    (0..corpus.len()).map(|i| corpus.note(i).0.to_owned()).collect()
}

/// Words of 4+ letters from evenly spaced notes: realistic queries for any vault.
fn sample_words(corpus: &impl TextCorpus) -> Vec<String> {
    let step = (corpus.len() / 40).max(1);
    (0..corpus.len())
        .step_by(step)
        .filter_map(|i| {
            let text = String::from_utf8_lossy(corpus.note(i).1).into_owned();
            let words: Vec<&str> = text.split(|c: char| !c.is_alphabetic()).filter(|w| w.chars().count() >= 4).collect();
            words.get(words.len() / 2).map(|w| w.to_lowercase())
        })
        .collect()
}

/// First three letters of a note title, as typed in a quick switcher.
fn title_prefix(path: &str) -> Option<String> {
    let title = path.rsplit('/').next()?.trim_end_matches(".md");
    let prefix: String = title.chars().take(3).collect();
    (!prefix.is_empty()).then_some(prefix)
}
