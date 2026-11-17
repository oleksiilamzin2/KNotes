use knotes_cards::{Fsrs, Grade, Memory};

fn near(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

#[test]
fn a_card_is_due_when_its_recall_has_fallen_to_the_retention() {
    let fsrs = Fsrs::default();
    assert!(near(fsrs.recall(10.0, 10.0), 0.9, 1e-9), "stability is the days to 90%");
    assert!(near(fsrs.recall(10.0, 0.0), 1.0, 1e-9));
    assert_eq!(fsrs.interval(10.0), 10.0, "due at 90%: after its stability");
    assert_eq!(fsrs.interval(0.2), 1.0, "a day at least");
    let keen = Fsrs { retention: 0.95, ..Fsrs::default() };
    assert!(keen.interval(10.0) < 10.0, "sooner for a higher retention");
}

#[test]
fn a_first_review_sets_the_memory_by_its_grade() {
    let fsrs = Fsrs::default();
    let first = |grade| fsrs.review(None, 0.0, grade);
    assert!(near(first(Grade::Good).stability, 3.173, 1e-9) && near(first(Grade::Easy).stability, 15.69105, 1e-9));
    assert!(first(Grade::Again).stability < first(Grade::Hard).stability);
    // Good: 7.1949 − e^(0.5345·2) + 1.
    assert!(near(first(Grade::Good).difficulty, 5.2824, 1e-3), "{}", first(Grade::Good).difficulty);
    assert!(first(Grade::Again).difficulty > first(Grade::Good).difficulty && first(Grade::Good).difficulty > first(Grade::Easy).difficulty);
}

#[test]
fn a_recall_strengthens_the_memory_and_a_lapse_weakens_it() {
    let fsrs = Fsrs::default();
    let before = Memory { stability: 10.0, difficulty: 5.0 };
    let after = |grade| fsrs.review(Some(before), 10.0, grade);
    assert!(after(Grade::Hard).stability > 10.0 && after(Grade::Good).stability > after(Grade::Hard).stability && after(Grade::Easy).stability > after(Grade::Good).stability);
    // Good at 90% recall: 10·(1 + e^1.54575·6·10^−0.1192·(e^(1.01925·0.1) − 1)).
    assert!(near(after(Grade::Good).stability, 32.96, 0.05), "{}", after(Grade::Good).stability);
    let lapsed = after(Grade::Again);
    assert!(lapsed.stability < 10.0 && lapsed.difficulty > 5.0, "{lapsed:?}");
    assert!(after(Grade::Easy).difficulty < 5.0 && near(after(Grade::Good).difficulty, 5.0, 0.05));
    // Later than due, the same recall strengthens more: it was the harder one.
    assert!(fsrs.review(Some(before), 30.0, Grade::Good).stability > after(Grade::Good).stability);
}

#[test]
fn the_same_day_again_is_a_short_step_and_the_memory_stays_in_its_bounds() {
    let fsrs = Fsrs::default();
    let before = Memory { stability: 3.0, difficulty: 5.0 };
    let again = fsrs.review(Some(before), 0.0, Grade::Again);
    let good = fsrs.review(Some(before), 0.0, Grade::Good);
    assert!(again.stability < 3.0 && good.stability > 3.0, "{again:?} {good:?}");
    let mut memory = fsrs.review(None, 0.0, Grade::Again);
    for _ in 0..50 {
        memory = fsrs.review(Some(memory), 1.0, Grade::Again);
    }
    assert!((1.0..=10.0).contains(&memory.difficulty) && memory.stability > 0.0, "{memory:?}");
    let mut memory = fsrs.review(None, 0.0, Grade::Easy);
    for _ in 0..50 {
        let days = fsrs.interval(memory.stability);
        memory = fsrs.review(Some(memory), days, Grade::Easy);
    }
    assert!((1.0..=10.0).contains(&memory.difficulty) && memory.stability.is_finite() && fsrs.interval(memory.stability) <= 36_500.0, "{memory:?}");
}

/// The memory and the intervals after `reviews` (the days since the last one, the grade).
fn follow(reviews: &[(f64, Grade)]) -> (Memory, Vec<f64>) {
    let fsrs = Fsrs::default();
    let mut memory = None;
    let mut intervals = Vec::new();
    for &(days, grade) in reviews {
        let after = fsrs.review(memory, days, grade);
        intervals.push(fsrs.interval(after.stability));
        memory = Some(after);
    }
    (memory.unwrap(), intervals)
}

/// The reference implementation's own test (py-fsrs 4.1.2, FSRS-5, `test_review_card`): Good six
/// times, Again twice, Good five times, each review when the card is due; its intervals are
/// 0, 4, 14, 44, 125, 328, 0, 0, 7, 16, 34, 71, 142 days. Its zeros are its learning steps (ten
/// minutes), which Knotes has not: the memory is the same, and the intervals where it has one.
#[test]
fn the_reference_s_intervals_are_ours() {
    use Grade::{Again, Good};
    let days = [0.0, 0.0, 4.0, 14.0, 44.0, 125.0, 328.0, 0.0, 0.0, 7.0, 16.0, 34.0, 71.0];
    let grades = [Good, Good, Good, Good, Good, Good, Again, Again, Good, Good, Good, Good, Good];
    let reviews: Vec<(f64, Grade)> = days.into_iter().zip(grades).collect();
    let (_, intervals) = follow(&reviews);
    assert_eq!(intervals[1..6], [4.0, 14.0, 44.0, 125.0, 328.0]);
    assert_eq!(intervals[8..], [7.0, 16.0, 34.0, 71.0, 142.0]);
}

/// The reference's `test_memo_state`: Again, then Good six times, 0, 0, 1, 3, 8 and 21 days
/// apart: a stability of 49 days (rounded) and a difficulty of 7.0866.
#[test]
fn the_reference_s_memory_is_ours() {
    use Grade::{Again, Good};
    let (memory, _) = follow(&[(0.0, Again), (0.0, Good), (0.0, Good), (1.0, Good), (3.0, Good), (8.0, Good), (21.0, Good)]);
    assert_eq!(memory.stability.round(), 49.0, "{memory:?}");
    assert_eq!((memory.difficulty * 10_000.0).round() / 10_000.0, 7.0866, "{memory:?}");
}

/// The reference counts whole days since the last review: a day and a half is a day.
#[test]
fn part_of_a_day_since_the_last_review_does_not_count() {
    let fsrs = Fsrs::default();
    let before = Some(fsrs.review(None, 0.0, Grade::Good));
    assert_eq!(fsrs.review(before, 1.9, Grade::Good), fsrs.review(before, 1.0, Grade::Good));
    assert_ne!(fsrs.review(before, 2.0, Grade::Good), fsrs.review(before, 1.0, Grade::Good));
}

/// A card forgotten keeps no more than a card recalled the same day would lose (the
/// reference's bound: its stability over e^(w17·w18)), not merely no more than it had.
#[test]
fn a_forgotten_card_s_stability_is_bounded_as_the_reference_bounds_it() {
    let fsrs = Fsrs::default();
    // Easy at once, then forgotten after a day: the long formula alone would give more.
    let weak = Memory { stability: 0.2, difficulty: 1.0 };
    let after = fsrs.review(Some(weak), 1.0, Grade::Again);
    let bound = 0.2 / (0.51655f64 * 0.6621).exp();
    assert!((after.stability - bound).abs() < 1e-9, "{after:?} {bound}");
}
