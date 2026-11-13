use std::path::Path;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bin/knotes/tests/fixtures/three-pages.pdf");

#[test]
fn a_pdf_s_text_is_its_pages_text() {
    let text = knotes_pdf::pdf_text(Path::new(FIXTURE)).unwrap();
    for page in ["Page 1", "Page 2", "Page 3"] {
        assert!(text.contains(page), "{text:?}");
    }
}

#[test]
fn a_file_that_is_no_pdf_has_no_text() {
    let dir = std::env::temp_dir().join("knotes-pdf-test-not-a.pdf");
    std::fs::write(&dir, "not a pdf").unwrap();
    assert_eq!(knotes_pdf::pdf_text(&dir), None);
    let _ = std::fs::remove_file(&dir);
}
