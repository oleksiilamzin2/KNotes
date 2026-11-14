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
