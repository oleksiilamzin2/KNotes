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

#[test]
fn a_vault_s_pdf_is_read_a_page_at_a_time() {
    use knotes_core::DocumentPages;
    let dir = std::path::Path::new(FIXTURE).parent().unwrap();
    let pages = knotes_pdf::PdfTexts::new(dir);
    assert_eq!(pages.page_count("three-pages.pdf"), Some(3));
    assert_eq!(pages.page_text("three-pages.pdf", 1).as_deref(), Some("Page 2"));
    assert_eq!(pages.page_count("note.md"), None);
    assert_eq!(pages.outline("outlined.pdf").first().map(|o| o.1.as_str()), Some("Вступ"));
}
