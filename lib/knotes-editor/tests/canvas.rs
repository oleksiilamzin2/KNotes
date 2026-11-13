use knotes_editor::Canvas;

fn runs(canvas: &Canvas<u8>) -> Vec<(std::ops::Range<usize>, u8)> {
    canvas.runs().collect()
}

#[test]
fn later_paint_covers_earlier_and_equal_neighbours_merge() {
    let mut canvas = Canvas::new(0..10, 0u8);
    canvas.paint(2..8, |v| *v = 1);
    canvas.paint(4..6, |v| *v = 2);
    canvas.paint(6..8, |v| *v = 0);
    assert_eq!(runs(&canvas), [(0..2, 0), (2..4, 1), (4..6, 2), (6..10, 0)]);
}

#[test]
fn paint_can_change_part_of_a_value() {
    let mut canvas = Canvas::new(0..4, 0u8);
    canvas.paint(0..4, |v| *v |= 1);
    canvas.paint(2..4, |v| *v |= 2);
    assert_eq!(runs(&canvas), [(0..2, 1), (2..4, 3)]);
}

#[test]
fn a_canvas_covers_part_of_the_text_and_clips_paint_to_it() {
    let mut canvas = Canvas::new(10..20, 0u8);
    canvas.paint(5..12, |v| *v = 1);
    canvas.paint(18..30, |v| *v = 2);
    canvas.paint(0..5, |v| *v = 3);
    assert_eq!(runs(&canvas), [(10..12, 1), (12..18, 0), (18..20, 2)]);
    assert_eq!(canvas.get(11), Some(&1));
    assert_eq!(canvas.get(25), None);
}

#[test]
fn an_empty_canvas_has_no_runs() {
    assert!(Canvas::new(3..3, 0u8).runs().next().is_none());
}

#[test]
fn runs_within_a_range_are_cut_to_it() {
    let mut canvas = Canvas::new(10..20, 0);
    canvas.paint(12..15, |u| *u = 1);
    canvas.paint(15..18, |u| *u = 2);
    let runs: Vec<_> = canvas.runs_in(13..17).collect();
    assert_eq!(runs, [(13..15, 1), (15..17, 2)]);
    assert_eq!(canvas.runs_in(5..11).collect::<Vec<_>>(), [(10..11, 0)], "cut to the canvas too");
    assert!(canvas.runs_in(25..30).next().is_none());
}
