use std::path::Path;
use std::sync::atomic::AtomicBool;

use knotes_pdf::find_in_pdf;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bin/knotes/tests/fixtures/three-pages.pdf");

#[test]
fn words_are_found_on_every_page_ignoring_case_as_character_ranges() {
    let found = find_in_pdf(Path::new(FIXTURE), "PAGE", &AtomicBool::new(false)).unwrap();
    assert_eq!(found, [(0, 0..4), (1, 0..4), (2, 0..4)]);
    let found = find_in_pdf(Path::new(FIXTURE), "ge 2", &AtomicBool::new(false)).unwrap();
    assert_eq!(found, [(1, 2..6)]);
    assert!(find_in_pdf(Path::new(FIXTURE), "nothing", &AtomicBool::new(false)).unwrap().is_empty());
}

#[test]
fn a_search_called_off_stops_and_an_empty_one_finds_nothing() {
    assert_eq!(find_in_pdf(Path::new(FIXTURE), "page", &AtomicBool::new(true)), None);
    assert_eq!(find_in_pdf(Path::new(FIXTURE), "", &AtomicBool::new(false)), Some(Vec::new()));
}
