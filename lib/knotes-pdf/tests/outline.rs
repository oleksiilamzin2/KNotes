use std::path::Path;

use knotes_pdf::{Bookmark, pdf_outline};

const OUTLINED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bin/knotes/tests/fixtures/outlined.pdf");
const PLAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bin/knotes/tests/fixtures/three-pages.pdf");

#[test]
fn a_pdf_s_bookmarks_come_in_order_with_their_level_page_and_height() {
    let outline = pdf_outline(Path::new(OUTLINED)).unwrap();
    let rounded: Vec<(u8, &str, usize, i64)> = outline.iter().map(|b: &Bookmark| (b.level, b.title.as_str(), b.page, (b.y * 100.0).round() as i64)).collect();
    // "Detail" goes to its page through a GoTo action, half way down.
    assert_eq!(rounded, [(1, "Вступ", 0, 0), (1, "Part", 1, 0), (2, "Detail", 2, 50)]);
}

#[test]
fn a_pdf_without_bookmarks_has_an_empty_outline() {
    assert_eq!(pdf_outline(Path::new(PLAIN)), Some(Vec::new()));
}
