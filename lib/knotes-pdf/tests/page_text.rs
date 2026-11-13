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
