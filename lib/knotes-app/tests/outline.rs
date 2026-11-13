use knotes_app::{Outline, OutlineEntry};

fn entry(level: u8, title: &str, at: usize) -> OutlineEntry {
    OutlineEntry { level, title: title.to_owned(), place: (at, 0.0) }
}

/// # A, ## B, ### C, ## D, # E, ### F (a level skipped).
fn outline() -> Outline {
    Outline::new(vec![entry(1, "A", 0), entry(2, "B", 10), entry(3, "C", 20), entry(2, "D", 30), entry(1, "E", 40), entry(3, "F", 50)])
}

#[test]
fn a_heading_sits_under_the_last_one_above_it_of_a_higher_level() {
    let outline = outline();
    assert_eq!(outline.children(None), [0, 4]);
    assert_eq!(outline.children(Some(0)), [1, 3]);
    assert_eq!(outline.children(Some(1)), [2]);
    assert_eq!(outline.children(Some(4)), [5], "a skipped level still nests");
    assert!(outline.children(Some(2)).is_empty());
    assert_eq!(outline.parent(2), Some(1));
    assert_eq!(outline.parent(4), None);
}

#[test]
fn headings_before_the_first_top_level_one_are_at_the_top() {
    let outline = Outline::new(vec![entry(2, "B", 0), entry(1, "A", 10), entry(2, "C", 20)]);
    assert_eq!(outline.children(None), [0, 1]);
    assert_eq!(outline.children(Some(1)), [2]);
}

#[test]
fn the_current_heading_is_the_last_one_at_or_above_the_top_of_the_view() {
    let outline = outline();
    assert_eq!(outline.current((0, 0.0)), Some(0));
    assert_eq!(outline.current((25, 0.0)), Some(2));
    assert_eq!(outline.current((30, 0.0)), Some(3));
    assert_eq!(outline.current((999, 0.0)), Some(5));
    assert_eq!(Outline::new(vec![entry(1, "A", 10)]).current((5, 0.0)), None, "above the first heading");
}

#[test]
fn a_place_in_a_pdf_is_its_page_then_how_far_down_it() {
    let pdf = Outline::new(vec![
        OutlineEntry { level: 1, title: "One".into(), place: (0, 0.1) },
        OutlineEntry { level: 1, title: "Two".into(), place: (0, 0.6) },
        OutlineEntry { level: 1, title: "Three".into(), place: (2, 0.0) },
    ]);
    assert_eq!(pdf.current((0, 0.5)), Some(0));
    assert_eq!(pdf.current((1, 0.9)), Some(1));
    assert_eq!(pdf.current((2, 0.3)), Some(2));
}

#[test]
fn a_heading_s_key_is_its_titles_from_the_top_and_stays_as_text_around_it_changes() {
    let before = outline();
    let after = Outline::new(vec![entry(1, "New", 0), entry(1, "A", 5), entry(2, "B", 15), entry(3, "C", 25)]);
    assert_eq!(before.key(2), after.key(3));
    assert_ne!(before.key(1), before.key(3));
}
