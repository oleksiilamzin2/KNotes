use knotes_core::{BlockHit, Span};
use knotes_embed::DIMS;
use knotes_vectors::VectorStore;

fn block(path: &str, ord: u32, heading: Option<&str>) -> BlockHit {
    BlockHit { path: path.into(), ord, span: Span { start: ord * 10, end: ord * 10 + 9 }, heading: heading.map(str::to_string), score: 0.0 }
}

#[test]
fn a_note_s_vectors_are_kept_with_its_date_and_come_back_as_a_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let store = VectorStore::open(dir.path(), "model-a").unwrap();
    store.put("a.md", 7, &[block("a.md", 0, None), block("a.md", 1, Some("Boiler"))], &[11, 12], &[[1; DIMS], [-2; DIMS]]).unwrap();
    store.put("b.md", 9, &[block("b.md", 0, None)], &[13], &[[3; DIMS]]).unwrap();
    assert_eq!(store.dates().unwrap(), [("a.md".to_string(), 7), ("b.md".to_string(), 9)].into());
    assert_eq!(store.vectors_of("a.md").unwrap(), [(11, [1; DIMS]), (12, [-2; DIMS])].into(), "each block's vector by the hash of its text");
    assert!(store.vectors_of("none.md").unwrap().is_empty());
    store.put("a.md", 8, &[block("a.md", 0, None)], &[14], &[[4; DIMS]]).unwrap();
    store.remove("b.md").unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot.nearest(&[4; DIMS], 5), [BlockHit { score: -f64::from(4 * 4 * DIMS as i32), ..block("a.md", 0, None) }]);
}

#[test]
fn vectors_of_another_model_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    VectorStore::open(dir.path(), "model-a").unwrap().put("a.md", 1, &[block("a.md", 0, Some("H"))], &[1], &[[1; DIMS]]).unwrap();
    assert_eq!(VectorStore::open(dir.path(), "model-a").unwrap().dates().unwrap().len(), 1, "the same model: kept");
    assert!(VectorStore::open(dir.path(), "model-b").unwrap().dates().unwrap().is_empty(), "another: embedded again");
}

#[test]
fn a_reader_sees_when_the_writer_has_changed_something() {
    let dir = tempfile::tempdir().unwrap();
    let writer = VectorStore::open(dir.path(), "m").unwrap();
    let reader = VectorStore::open_reader(dir.path()).unwrap();
    let before = reader.version().unwrap();
    assert_eq!(reader.version().unwrap(), before, "nothing written");
    writer.put("a.md", 1, &[block("a.md", 0, None)], &[1], &[[1; DIMS]]).unwrap();
    assert_ne!(reader.version().unwrap(), before);
    assert_eq!(reader.snapshot().unwrap().len(), 1);
}
