use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use knotes_embed::{Embedder, Passage, ProcessEmbedder, similarity};

/// The model, once `scripts/setup.sh` has fetched it: a test without it says so and is skipped,
/// so that `cargo test --workspace` passes on a machine without the models.
fn model() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/embeddinggemma-2-Q8_0.gguf");
    if !path.exists() {
        eprintln!("skipped: no model at {} (scripts/setup.sh fetches it)", path.display());
        return None;
    }
    Some(path)
}

#[test]
fn the_helper_embeds_what_it_is_sent_over_its_pipes() {
    let Some(model) = model() else { return };
    let embedder = ProcessEmbedder::new(env!("CARGO_BIN_EXE_knotes-embed"), model);
    let passages = [
        Passage::Query("коли міняти анод у бойлері".into()),
        Passage::Document { title: "Бойлер".into(), text: "Обслуговування › Магнієвий анод замінювати щороку, восени.".into() },
        Passage::Document { title: "Кухня".into(), text: "Меню на тиждень: понеділок — борщ.".into() },
    ];
    let v = embedder.embed(&passages).unwrap();
    assert!(similarity(&v[0], &v[1]) > similarity(&v[0], &v[2]));
}

#[test]
fn a_missing_model_is_said_in_each_answer() {
    let embedder = ProcessEmbedder::new(env!("CARGO_BIN_EXE_knotes-embed"), "/nonexistent/model.gguf");
    let error = embedder.embed(&[Passage::Query("x".into())]).unwrap_err().to_string();
    assert!(error.contains("/nonexistent/model.gguf"), "{error}");
}

#[test]
fn the_helper_exits_once_idle() {
    let Some(model) = model() else { return };
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes-embed"))
        .arg(model)
        .env("KNOTES_EMBED_IDLE_MS", "200")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < Duration::from_secs(20), "still running");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(child.wait().unwrap().success());
}
