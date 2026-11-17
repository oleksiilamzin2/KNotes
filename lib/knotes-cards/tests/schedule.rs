use knotes_cards::{Fsrs, Grade, parse_review, review_line, schedule};

const DAY: i64 = 86_400;

#[test]
fn a_schedule_follows_a_card_s_reviews_in_order() {
    let fsrs = Fsrs::default();
    assert_eq!(schedule(&fsrs, []), None, "never reviewed");
    let first = schedule(&fsrs, [(1_000, Grade::Good)]).unwrap();
    assert_eq!((first.last, first.due, first.reviews, first.lapses), (1_000, 1_000 + 3 * DAY, 1, 0), "Good first: three days");
    let two = schedule(&fsrs, [(1_000, Grade::Good), (1_000 + 3 * DAY, Grade::Good)]).unwrap();
    assert!(two.due - two.last > 3 * DAY && two.memory.stability > first.memory.stability && two.reviews == 2);
    let lapsed = schedule(&fsrs, [(1_000, Grade::Good), (1_000 + 3 * DAY, Grade::Again)]).unwrap();
    assert_eq!((lapsed.due, lapsed.lapses), (1_000 + 3 * DAY + 600, 1), "Again: back in ten minutes");
    assert_eq!(schedule(&fsrs, [(1_000, Grade::Again)]).unwrap().lapses, 0, "not a lapse before it was learned");
}

#[test]
fn a_review_is_logged_as_a_line_and_read_back() {
    let line = review_line(1_763_370_000, "c-a1b2c3", Grade::Hard);
    assert_eq!(line, "1763370000\tc-a1b2c3\t2\n");
    assert_eq!(parse_review(&line), Some((1_763_370_000, "c-a1b2c3", Grade::Hard)));
    for damaged in ["", "1763370000\tc-1", "x\tc-1\t3", "1\tc-1\t5", "1\t\t3", "1\tc-1\t3\textra"] {
        assert_eq!(parse_review(damaged), None, "{damaged:?}");
    }
}
