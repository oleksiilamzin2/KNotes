use knotes_app::{bar_length, bar_under, scale_top, share};

#[test]
fn a_scale_s_top_is_a_round_number_no_less_than_its_largest_value() {
    let tops: Vec<usize> = [0, 1, 2, 3, 5, 6, 10, 11, 20, 21, 50, 51, 100, 999, 1_000, 1_001, 100_000].into_iter().map(scale_top).collect();
    assert_eq!(tops, [1, 1, 2, 5, 5, 10, 10, 20, 20, 50, 50, 100, 100, 1_000, 1_000, 2_000, 100_000]);
}

#[test]
fn a_bar_is_as_long_as_its_share_and_a_small_one_still_shows() {
    assert_eq!((bar_length(110.0, 50, 100), bar_length(110.0, 100, 100), bar_length(110.0, 0, 100)), (55.0, 110.0, 0.0));
    assert_eq!(bar_length(110.0, 1, 1_000), 2.0, "one in a thousand is two points, not a tenth of one");
    assert_eq!(bar_length(110.0, 0, 0), 0.0, "no scale: nothing, and nothing divided by nothing");
}

#[test]
fn the_pointer_in_a_bar_s_column_is_on_that_bar() {
    // 30 bars over 300 points from 30: ten points each.
    assert_eq!((bar_under(30.0, 30.0, 300.0, 30), bar_under(39.9, 30.0, 300.0, 30), bar_under(40.0, 30.0, 300.0, 30)), (Some(0), Some(0), Some(1)));
    assert_eq!(bar_under(329.99, 30.0, 300.0, 30), Some(29));
    assert_eq!((bar_under(29.9, 30.0, 300.0, 30), bar_under(330.0, 30.0, 300.0, 30)), (None, None), "left of the plot, and past it");
    assert_eq!((bar_under(50.0, 30.0, 300.0, 0), bar_under(50.0, 30.0, 0.0, 5)), (None, None));
}

#[test]
fn a_share_is_rounded_to_the_nearest_per_cent() {
    assert_eq!((share(1, 3), share(2, 3), share(1, 200), share(1, 201), share(7, 7), share(0, 7)), (Some(33), Some(67), Some(1), Some(0), Some(100), Some(0)));
    assert_eq!(share(0, 0), None, "no cards, no share");
}
