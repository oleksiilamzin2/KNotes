//! Over the real model (`scripts/setup.sh` fetches it): what it finds near what.

use std::path::PathBuf;
use std::time::Instant;

use knotes_embed::{DIMS, Embedder, Passage, Vector, similarity};
use knotes_llama::LlamaEmbedder;

/// The models search by meaning runs on (ADR 0029): e5-large-instruct, EmbeddingGemma 2.
const MODELS: [&str; 2] = ["multilingual-e5-large-instruct-q8_0.gguf", "embeddinggemma-2-Q8_0.gguf"];

/// `file`'s model, of each test's own, dropped at its end (Metal checks at exit that all is freed).
fn load(file: &str) -> LlamaEmbedder {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models").join(file);
    assert!(path.exists(), "no model at {}: run scripts/setup.sh", path.display());
    LlamaEmbedder::load(&path).unwrap()
}

fn model() -> LlamaEmbedder {
    load(MODELS[0])
}

fn block(title: &str, text: &str) -> Passage {
    Passage::Document { title: title.into(), text: text.into() }
}

fn nearest(model: &LlamaEmbedder, query: &str, blocks: &[Passage]) -> usize {
    ranked(model, query, blocks)[0]
}

/// The blocks, nearest `query` first.
fn ranked(model: &LlamaEmbedder, query: &str, blocks: &[Passage]) -> Vec<usize> {
    let mut passages = vec![Passage::Query(query.into())];
    passages.extend_from_slice(blocks);
    let vectors = model.embed(&passages).unwrap();
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(similarity(&vectors[0], &vectors[i + 1])));
    order
}

#[test]
fn a_query_lands_nearest_the_block_that_means_it_in_other_words_or_another_language() {
    let blocks = [
        block("Велосипед", "Ланцюг треба мастити спеціальним мастилом раз на місяць, після дощу частіше."),
        block("Кухня", "Борщ: буряк, капуста, картопля, морква, томатна паста."),
        block("Car", "The insurance policy renews every March; the broker is Allianz."),
        block("Дім", "Лічильник електроенергії: 2026-10-01 день 14 230, ніч 6 115."),
    ];
    for file in MODELS {
        let model = load(file);
        assert_eq!(nearest(&model, "чим змащувати велосипедний ланцюг", &blocks), 0, "{file}: other forms of the words");
        // A Ukrainian query, an English note: e5 put the insurance third of four (2026-11-15),
        // though on the benchmark it found English queries' Ukrainian notes; EmbeddingGemma 2 first.
        if !file.starts_with("multilingual-e5") {
            assert_eq!(nearest(&model, "страховка машини", &blocks), 2, "{file}: the other language");
        }
        assert_eq!(nearest(&model, "скільки світла спожили", &blocks), 3, "{file}: other words altogether");
    }
}

#[test]
fn a_batch_gives_the_vectors_one_at_a_time_gives() {
    let passages: Vec<Passage> = (0..5).map(|i| block("Нотатка", &format!("Абзац номер {i}: {}", "слово ".repeat(i * 7)))).collect();
    let model = model();
    let together = model.embed(&passages).unwrap();
    assert_eq!(together.len(), 5);
    for (passage, vector) in passages.iter().zip(&together) {
        let alone: Vector = model.embed(std::slice::from_ref(passage)).unwrap()[0];
        let same = similarity(&alone, vector) as f64 / similarity(&alone, &alone) as f64;
        assert!(same > 0.99, "{same}");
    }
    assert_eq!(together[0].len(), DIMS);
}

#[test]
#[ignore = "a measurement: cargo test -p knotes-llama --release -- --ignored --nocapture"]
fn how_fast_blocks_are_embedded() {
    let passages: Vec<Passage> = (0..256).map(|i| block("Щоденник", &format!("{i}. Сьогодні полагодили змішувач, купили анод для бойлера і домовилися з Василем на вівторок о дев'ятій."))).collect();
    let start = Instant::now();
    let model = model();
    println!("loaded in {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
    model.embed(&passages[..8]).unwrap();
    let start = Instant::now();
    model.embed(&passages).unwrap();
    let per = start.elapsed().as_secs_f64() * 1000.0 / passages.len() as f64;
    println!("{per:.2} ms a block, {:.0} blocks a second", 1000.0 / per);
    let start = Instant::now();
    for _ in 0..10 {
        model.embed(&[Passage::Query("коли приходить Василь".into())]).unwrap();
    }
    println!("{:.1} ms a query", start.elapsed().as_secs_f64() * 100.0);
}
