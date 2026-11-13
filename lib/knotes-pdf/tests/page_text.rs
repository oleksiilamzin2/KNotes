use knotes_pdf::{Bounds, PageText};

/// `ab cd` on a line at y 0.1–0.2 and `ef` on one at 0.3–0.4, characters 0.1 wide from
/// x 0.1; the line break has the box of the character before it, as PDFKit gives it.
fn page() -> PageText {
    let glyph = |ch, x: f64, y: f64| (ch, Bounds { x, y, width: 0.1, height: 0.1 });
    PageText::new([
        glyph('a', 0.1, 0.1),
        glyph('b', 0.2, 0.1),
        glyph(' ', 0.3, 0.1),
        glyph('c', 0.4, 0.1),
        glyph('d', 0.5, 0.1),
        glyph('\n', 0.5, 0.1),
        glyph('e', 0.1, 0.3),
        glyph('f', 0.2, 0.3),
    ])
}

#[test]
fn the_caret_goes_before_the_character_whose_middle_is_right_of_the_point() {
    let page = page();
    assert_eq!(page.caret_at(0.24, 0.15), 1);
    assert_eq!(page.caret_at(0.26, 0.15), 2);
    assert_eq!(page.caret_at(0.0, 0.15), 0, "left of the line: its start");
    assert_eq!(page.caret_at(0.9, 0.15), 5, "right of the line: its end, before the break");
    assert_eq!(page.caret_at(0.16, 0.35), 7);
}

#[test]
fn a_point_between_or_beyond_the_lines_takes_the_nearest_line() {
    let page = page();
    assert_eq!(page.caret_at(0.0, 0.22), 0);
    assert_eq!(page.caret_at(0.0, 0.28), 6);
    assert_eq!(page.caret_at(0.9, 0.9), 8);
    assert_eq!(page.caret_at(0.0, 0.0), 0);
}

#[test]
fn of_two_lines_side_by_side_the_point_takes_the_one_it_is_over() {
    let glyph = |ch, x: f64| (ch, Bounds { x, y: 0.1, width: 0.1, height: 0.1 });
    let columns = PageText::new([glyph('a', 0.1), glyph('b', 0.2), glyph('\n', 0.2), glyph('c', 0.6), glyph('d', 0.7)]);
    assert_eq!(columns.caret_at(0.66, 0.15), 4);
    assert_eq!(columns.caret_at(0.22, 0.15), 1);
}

#[test]
fn a_range_s_text_keeps_its_line_breaks() {
    assert_eq!(page().text(1..7), "b cd\ne");
    assert_eq!(page().text(0..8), "ab cd\nef");
}

#[test]
fn a_word_is_the_letters_and_digits_around_the_point_and_anything_else_is_itself() {
    let page = page();
    assert_eq!(page.word_at(0.42, 0.15), 3..5);
    assert_eq!(page.word_at(0.32, 0.15), 2..3);
    assert_eq!(page.word_at(0.9, 0.15), 3..5, "right of the line: its last word");
}

#[test]
fn a_line_is_its_characters_without_the_break() {
    assert_eq!(page().line_at(0.12, 0.35), 6..8);
    assert_eq!(page().line_at(0.6, 0.12), 0..5);
}

/// Boxes as hundredths, for comparing.
fn hundredths(boxes: Vec<Bounds>) -> Vec<[i64; 4]> {
    let h = |v: f64| (v * 100.0).round() as i64;
    boxes.iter().map(|b| [h(b.x), h(b.y), h(b.width), h(b.height)]).collect()
}

#[test]
fn a_range_is_highlighted_line_by_line() {
    assert_eq!(hundredths(page().highlight(1..7)), [[20, 10, 40, 10], [10, 30, 10, 10]]);
    assert!(page().highlight(3..3).is_empty());
}

#[test]
fn characters_with_no_place_are_not_hit_or_highlighted() {
    let page = PageText::new([
        ('a', Bounds { x: 0.1, y: 0.1, width: 0.1, height: 0.1 }),
        ('b', Bounds::default()),
        ('c', Bounds { x: 0.3, y: 0.1, width: 0.1, height: 0.1 }),
    ]);
    assert_eq!(page.caret_at(0.31, 0.15), 2);
    assert!(page.highlight(1..2).is_empty());
    assert_eq!(hundredths(page.highlight(0..3)), [[10, 10, 30, 10]]);
}

#[test]
fn a_box_in_pdf_space_is_placed_on_the_page_as_shown_turned() {
    let crop = Bounds { x: 0.0, y: 0.0, width: 200.0, height: 100.0 };
    let glyph = Bounds { x: 42.0, y: 40.0, width: 14.0, height: 17.0 };
    assert_eq!(hundredths(vec![glyph.shown(crop, 0)]), [[21, 43, 7, 17]]);
    assert_eq!(hundredths(vec![glyph.shown(crop, 90)]), [[40, 21, 17, 7]]);
    assert_eq!(hundredths(vec![glyph.shown(crop, 180)]), [[72, 40, 7, 17]]);
    assert_eq!(hundredths(vec![glyph.shown(crop, 270)]), [[43, 72, 17, 7]]);
    assert_eq!(hundredths(vec![glyph.shown(crop, -90)]), [[43, 72, 17, 7]]);
    let cropped = Bounds { x: 20.0, y: 10.0, width: 100.0, height: 50.0 };
    assert_eq!(hundredths(vec![glyph.shown(cropped, 0)]), [[22, 6, 14, 34]]);
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bin/knotes/tests/fixtures/three-pages.pdf");

#[test]
fn a_page_s_text_comes_with_where_its_characters_are() {
    let page = knotes_pdf::page_text(std::path::Path::new(FIXTURE), 1).unwrap();
    assert_eq!(page.text(0..page.len()), "Page 2");
    // "Page 2" in 24-point Helvetica at 42, 40 on a 200 by 100 page.
    let line = page.highlight(0..page.len());
    assert_eq!(line.len(), 1);
    assert!((0.19..0.23).contains(&line[0].x) && (0.3..0.5).contains(&line[0].y), "{line:?}");
    assert_eq!(page.word_at(0.3, 0.5), 0..4);
    assert!(knotes_pdf::page_text(std::path::Path::new(FIXTURE), 3).is_none(), "no fourth page");
}
