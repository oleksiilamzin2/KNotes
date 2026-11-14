use std::sync::Arc;

use knotes_core::{BlockHit, FileKind, IndexChange, IndexWriter, MeaningSearch, NoteParser, Span, VaultFile};
use knotes_embed::{DIMS, EmbedError, Embedder, Passage, Vector, quantize};
use knotes_index::SqliteIndex;
use knotes_md::MarkdownParser;
use knotes_vectors::{MeaningIndex, Snapshot, block_passage, embed_notes};

/// Words as vectors: each word lights the dimension its letters sum to, so texts sharing words
/// are near.
struct Words;

impl Embedder for Words {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        Ok(passages
            .iter()
            .map(|p| {
                let text = match p {
                    Passage::Query(q) => q.clone(),
                    Passage::Document { text, .. } => text.clone(),
                };
                let mut v = vec![0.0f32; DIMS];
                for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2) {
                    v[word.chars().map(|c| c as usize).sum::<usize>() % DIMS] += 1.0;
                }
                quantize(&v)
            })
            .collect())
    }
}

fn hit(path: &str, ord: u32, start: u32, end: u32, heading: Option<&str>) -> BlockHit {
    BlockHit { path: path.into(), ord, span: Span { start, end }, heading: heading.map(str::to_string), score: 0.0 }
}

#[test]
fn a_block_is_embedded_with_its_note_s_title_and_its_section_s_heading() {
    let text = "# Boiler\n\nService it yearly.\n\nThe anode is magnesium.";
    let first = block_passage("Home/Boiler.md", text, &hit("Home/Boiler.md", 0, 0, 28, Some("Boiler")));
    assert_eq!(first, Passage::Document { title: "Boiler".into(), text: "# Boiler\n\nService it yearly.".into() }, "the heading is in its text already");
    let second = block_passage("Home/Boiler.md", text, &hit("Home/Boiler.md", 1, 30, 53, Some("Boiler")));
    assert_eq!(second, Passage::Document { title: "Boiler".into(), text: "Boiler › The anode is magnesium.".into() });
    let plain = block_passage("Scan.pdf", "Page one.", &hit("Scan.pdf", 0, 0, 9, None));
    assert_eq!(plain, Passage::Document { title: "Scan".into(), text: "Page one.".into() });
}

#[test]
fn the_nearest_blocks_come_first() {
    let mut snapshot = Snapshot::default();
    let vectors = Words.embed(&[Passage::Query("boiler anode".into()), Passage::Query("garden roses".into()), Passage::Query("boiler pressure".into())]).unwrap();
    for (i, v) in vectors.into_iter().enumerate() {
        snapshot.push(hit(&format!("{i}.md"), 0, 0, 1, None), v);
    }
    let query = Words.embed(&[Passage::Query("anode of the boiler".into())]).unwrap()[0];
    let near: Vec<String> = snapshot.nearest(&query, 2).into_iter().map(|h| h.path).collect();
    assert_eq!(near, ["0.md", "2.md"]);
    assert!(snapshot.nearest(&query, 3)[0].score < snapshot.nearest(&query, 3)[2].score, "lower the nearer");
    assert_eq!(snapshot.len(), 3);
}

#[test]
fn the_vault_s_blocks_are_embedded_and_searched_by_meaning() {
    let dir = tempfile::tempdir().unwrap();
    let mut index = SqliteIndex::open(dir.path()).unwrap();
    let notes = [("Home/Boiler.md", "# Boiler\n\nReplace the magnesium anode every autumn."), ("Garden.md", "Roses want pruning in March.")];
    let parsed: Vec<_> = notes.iter().map(|(_, t)| MarkdownParser::new().parse(t)).collect();
    let files: Vec<_> = notes.iter().map(|(p, _)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
    let changes: Vec<_> = files.iter().zip(&notes).zip(&parsed).map(|((file, (_, text)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
    index.apply(&changes).unwrap();
    let snapshot = embed_notes(&index, &Words).unwrap();
    assert_eq!(snapshot.len(), 2);
    let meaning = MeaningIndex::new(Arc::new(Words), snapshot);
    let found = meaning.nearest("when to replace the anode", 1).unwrap();
    assert_eq!((found[0].path.as_str(), found[0].heading.as_deref()), ("Home/Boiler.md", Some("Boiler")));
}

fn index_notes(index: &mut SqliteIndex, notes: &[(&str, &str, i64)]) {
    let parsed: Vec<_> = notes.iter().map(|(_, t, _)| MarkdownParser::new().parse(t)).collect();
    let files: Vec<_> = notes.iter().map(|(p, _, m)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: *m }).collect();
    let changes: Vec<_> = files.iter().zip(notes).zip(&parsed).map(|((file, (_, text, _)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
    index.apply(&changes).unwrap();
}

/// The words embedder, counting the passages it is given.
#[derive(Default)]
struct Counting(std::sync::atomic::AtomicUsize);

impl Embedder for Counting {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        self.0.fetch_add(passages.len(), std::sync::atomic::Ordering::Relaxed);
        Words.embed(passages)
    }
}

impl Counting {
    fn take(&self) -> usize {
        self.0.swap(0, std::sync::atomic::Ordering::Relaxed)
    }
}

/// Long after every date the tests give.
const LATER: i64 = 1_000_000_000_000;

#[test]
fn only_documents_changed_since_they_were_embedded_are_embedded_again() {
    use std::sync::atomic::AtomicBool;
    use knotes_vectors::{VectorStore, Which, update};
    let (idx, vectors) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    index_notes(&mut index, &[("Boiler.md", "Replace the anode.", 1), ("Garden.md", "Prune the roses.", 1)]);
    let store = VectorStore::open(vectors.path(), "words").unwrap();
    let (go, model) = (AtomicBool::new(false), Counting::default());
    assert_eq!(update(&index, &store, &model, LATER, Which::Settled, &go).unwrap().documents, 2);
    assert_eq!(update(&index, &store, &model, LATER, Which::Settled, &go).unwrap().documents, 0, "nothing changed");
    index_notes(&mut index, &[("Garden.md", "Prune the roses and the apple trees.", 2)]);
    index.apply(&[IndexChange::Remove { path: "Boiler.md" }]).unwrap();
    assert_eq!(update(&index, &store, &model, LATER, Which::Settled, &go).unwrap().documents, 1, "the changed one");
    assert_eq!(store.dates().unwrap(), [("Garden.md".to_string(), 2)].into(), "the removed one gone");
    let stop = AtomicBool::new(true);
    index_notes(&mut index, &[("New.md", "Fresh.", 3)]);
    assert_eq!(update(&index, &store, &model, LATER, Which::Settled, &stop).unwrap().documents, 0, "told to stop");

    let meaning = MeaningIndex::over_store(Arc::new(Words), VectorStore::open_reader(vectors.path()).unwrap());
    assert_eq!(meaning.nearest("apple trees", 1).unwrap()[0].path, "Garden.md");
}

#[test]
fn only_the_blocks_whose_text_changed_go_to_the_model() {
    use std::sync::atomic::AtomicBool;
    use knotes_vectors::{VectorStore, Which, update};
    let (idx, vectors) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    let three = "# Boiler\n\nReplace the anode.\n\n# Garden\n\nPrune the roses.\n\n# Car\n\nChange the oil.";
    index_notes(&mut index, &[("Home.md", three, 1)]);
    let store = VectorStore::open(vectors.path(), "words").unwrap();
    let (go, model) = (AtomicBool::new(false), Counting::default());
    update(&index, &store, &model, LATER, Which::Settled, &go).unwrap();
    assert_eq!(model.take(), 3);
    index_notes(&mut index, &[("Home.md", &three.replace("roses", "apple trees"), 2)]);
    let updated = update(&index, &store, &model, LATER, Which::Settled, &go).unwrap();
    assert_eq!((updated.documents, updated.passages, model.take()), (1, 1, 1), "the garden's block alone");
    let meaning = MeaningIndex::over_store(Arc::new(Words), VectorStore::open_reader(vectors.path()).unwrap());
    assert_eq!(meaning.nearest("change the oil", 1).unwrap()[0].heading.as_deref(), Some("Car"), "the kept vectors still found");
}

#[test]
fn a_document_still_being_written_waits_until_it_settles() {
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use knotes_vectors::{SETTLE, VectorStore, Which, update};
    let (idx, vectors) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    let now: i64 = 5_000_000_000_000;
    let second = 1_000_000_000;
    index_notes(&mut index, &[("Old.md", "Settled long ago.", 1), ("Typing.md", "Being written", now - 5 * second)]);
    let store = VectorStore::open(vectors.path(), "words").unwrap();
    let (go, model) = (AtomicBool::new(false), Counting::default());
    let updated = update(&index, &store, &model, now, Which::Settled, &go).unwrap();
    assert_eq!(updated.documents, 1, "the old one only");
    assert_eq!(updated.next, Some(SETTLE - Duration::from_secs(5)), "when the one being written is due");
    let updated = update(&index, &store, &model, now + SETTLE.as_nanos() as i64, Which::Settled, &go).unwrap();
    assert_eq!((updated.documents, updated.next), (1, None), "settled: embedded");
    assert_eq!(SETTLE, Duration::from_secs(15));
}

#[test]
fn before_the_chat_asks_the_documents_being_written_are_embedded_at_once() {
    use std::sync::atomic::AtomicBool;
    use knotes_vectors::{VectorStore, Which, update};
    let (idx, vectors) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut index = SqliteIndex::open(idx.path()).unwrap();
    let now: i64 = 5_000_000_000_000;
    index_notes(&mut index, &[("Backlog.md", "Never embedded, long settled.", 1), ("Typing.md", "Just written", now - 1_000_000_000)]);
    let store = VectorStore::open(vectors.path(), "words").unwrap();
    let (go, model) = (AtomicBool::new(false), Counting::default());
    let updated = update(&index, &store, &model, now, Which::Waiting, &go).unwrap();
    assert_eq!((updated.documents, updated.next), (1, None));
    assert_eq!(store.dates().unwrap().keys().collect::<Vec<_>>(), ["Typing.md"], "the one just written; the backlog stays the background's");
}
