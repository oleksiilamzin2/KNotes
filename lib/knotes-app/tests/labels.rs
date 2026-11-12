use knotes_app::{Clock, TimeStyle, plural_uk, time_style};

const H: i64 = 3_600_000_000_000;
const D: i64 = 24 * H;
/// 2026-11-12 10:00 in UTC+2 (a Thursday).
const NOW: i64 = 1_794_470_400_000_000_000;
const CLOCK: Clock = Clock { now_ns: NOW, utc_offset_s: 7200 };

#[test]
fn time_style_like_apple_notes() {
    assert_eq!(time_style(NOW - H, &CLOCK), TimeStyle::Time);
    assert_eq!(time_style(NOW - 10 * H - 1, &CLOCK), TimeStyle::Yesterday, "just before local midnight");
    assert_eq!(time_style(NOW - 3 * D, &CLOCK), TimeStyle::Weekday);
    assert_eq!(time_style(NOW - 6 * D, &CLOCK), TimeStyle::Weekday);
    assert_eq!(time_style(NOW - 7 * D, &CLOCK), TimeStyle::Date);
    assert_eq!(time_style(NOW + H, &CLOCK), TimeStyle::Time, "clock skew counts as today");
}

#[test]
fn ukrainian_plurals() {
    let forms = |n| plural_uk(n, "нотатка", "нотатки", "нотаток");
    let cases = [(0, "нотаток"), (1, "нотатка"), (2, "нотатки"), (4, "нотатки"), (5, "нотаток"), (11, "нотаток"), (14, "нотаток"), (21, "нотатка"), (22, "нотатки"), (111, "нотаток"), (50000, "нотаток")];
    for (n, form) in cases {
        assert_eq!(forms(n), form, "{n}");
    }
}
