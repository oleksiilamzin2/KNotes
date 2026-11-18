use knotes_cards::{Fsrs, Grade, Logged, Schedule, Steps, parse_logged, parse_review, review, review_line, schedule, taken_back_line};

const DAY: i64 = 86_400;

#[test]
fn a_schedule_follows_a_card_s_reviews_in_order() {
    let (fsrs, none) = (Fsrs::default(), Steps::of_minutes(&[]));
    assert_eq!(schedule(&fsrs, &none, []), None, "never reviewed");
    let first = schedule(&fsrs, &none, [(1_000, Grade::Good)]).unwrap();
    assert_eq!((first.last, first.due, first.reviews, first.lapses, first.step), (1_000, 1_000 + 3 * DAY, 1, 0, None), "no steps: Good first, three days");
    let two = schedule(&fsrs, &none, [(1_000, Grade::Good), (1_000 + 3 * DAY, Grade::Good)]).unwrap();
    assert!(two.due - two.last > 3 * DAY && two.memory.stability > first.memory.stability && two.reviews == 2);
    let lapsed = schedule(&fsrs, &none, [(1_000, Grade::Good), (1_000 + 3 * DAY, Grade::Again)]).unwrap();
    assert_eq!((lapsed.due, lapsed.lapses, lapsed.step, lapsed.relearning), (1_000 + 3 * DAY + 600, 1, Some(0), true), "forgotten: learned again, back in ten minutes");
    assert_eq!(schedule(&fsrs, &none, [(1_000, Grade::Again)]).unwrap().lapses, 0, "not a lapse at its first review");
}

/// The reference's own test with its steps (py-fsrs 4.1.2, `test_review_card`): each review when
/// the card is due; the whole days to the next are 0, 4, 14, 44, 125, 328, 0, 0, 7, 16, 34, 71, 142.
#[test]
fn the_reference_s_intervals_with_its_steps_are_ours() {
    use Grade::{Again, Good};
    let (fsrs, steps) = (Fsrs::default(), Steps::default());
    let mut at: Option<Schedule> = None;
    let mut when = 1_669_725_000;
    let mut days = Vec::new();
    for grade in [Good, Good, Good, Good, Good, Good, Again, Again, Good, Good, Good, Good, Good] {
        let after = review(&fsrs, &steps, at.as_ref(), when, grade);
        days.push((after.due - after.last) / DAY);
        when = after.due;
        at = Some(after);
    }
    assert_eq!(days, [0, 4, 14, 44, 125, 328, 0, 0, 7, 16, 34, 71, 142]);
}

#[test]
fn a_new_card_is_learned_in_steps_before_its_days_begin() {
    let (fsrs, steps) = (Fsrs::default(), Steps::default());
    let after = |before: Option<&Schedule>, grade| review(&fsrs, &steps, before, 1_000, grade);
    let wait = |s: Schedule| (s.due - s.last, s.step, s.relearning);
    // A new card: a minute, between its two steps, ten minutes, or its days at once.
    assert_eq!(wait(after(None, Grade::Again)), (60, Some(0), false));
    assert_eq!(wait(after(None, Grade::Hard)), (330, Some(0), false));
    assert_eq!(wait(after(None, Grade::Good)), (600, Some(1), false));
    assert_eq!(wait(after(None, Grade::Easy)), (16 * DAY, None, false));
    // At its last step: Good begins its days, Hard keeps it there, Again starts over.
    let last = after(None, Grade::Good);
    assert_eq!(wait(after(Some(&last), Grade::Good)).1, None);
    assert_eq!(wait(after(Some(&last), Grade::Hard)), (600, Some(1), false));
    assert_eq!(wait(after(Some(&last), Grade::Again)), (60, Some(0), false));
    // Forgotten after its days began: ten minutes, then Good begins them again.
    let learned = after(Some(&last), Grade::Good);
    let forgotten = after(Some(&learned), Grade::Again);
    assert_eq!(wait(forgotten), (600, Some(0), true));
    assert_eq!(wait(after(Some(&forgotten), Grade::Hard)), (900, Some(0), true), "one step: half as long again");
    assert_eq!(wait(after(Some(&forgotten), Grade::Good)).1, None);
    // One step, and steps taken away while a card waited at a later one.
    let one = Steps::of_minutes(&[5]);
    assert_eq!(review(&fsrs, &one, None, 0, Grade::Hard).due, 450);
    assert_eq!(review(&fsrs, &one, None, 0, Grade::Good).step, None);
    assert_eq!(review(&fsrs, &one, Some(&last), 1_000, Grade::Hard).step, None, "its step is no more: its days begin");
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

#[test]
fn a_review_is_taken_back_by_a_line_after_it() {
    let line = taken_back_line(1_763_370_000, "c-a1b2c3");
    assert_eq!(line, "1763370000\tc-a1b2c3\t0\n");
    assert_eq!(parse_logged(&line), Some(Logged::TakenBack(1_763_370_000, "c-a1b2c3")));
    assert_eq!(parse_review(&line), None, "no review itself: an older Knotes passes it over");
    assert_eq!(parse_logged(&review_line(5, "c-1", Grade::Good)), Some(Logged::Review(5, "c-1", Grade::Good)));
    // A card paused, and resumed.
    assert_eq!(knotes_cards::paused_line(7, "c-1", true), "7\tc-1\tP\n");
    assert_eq!(parse_logged(&knotes_cards::paused_line(7, "c-1", true)), Some(Logged::Paused(7, "c-1", true)));
    assert_eq!(parse_logged(&knotes_cards::paused_line(9, "c-1", false)), Some(Logged::Paused(9, "c-1", false)));
    assert_eq!((parse_logged("7\tc-1\tp"), parse_logged("7\t\tP"), parse_logged("7\tc-1\tP\tx")), (None, None, None));
    for damaged in ["", "1\tc-1", "x\tc-1\t0", "1\t\t0", "1\tc-1\t0\textra", "1\tc-1\t5"] {
        assert_eq!(parse_logged(damaged), None, "{damaged:?}");
    }
}
