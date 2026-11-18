//! The numbers of a bar chart (a deck's statistics, ADR 0037): the top of its scale, how long a
//! bar is, which bar is under the pointer, a part's share.

/// The top of a plot's scale for its largest value: 1, 2 or 5 times a power of ten.
pub fn scale_top(most: usize) -> usize {
    let mut power = 1;
    loop {
        for times in [1, 2, 5] {
            if times * power >= most.max(1) {
                return times * power;
            }
        }
        power *= 10;
    }
}

/// How long the bar of `value` is on a scale of `top` drawn `room` points long: its share of
/// the room, and two points at least for a value that is not nothing, so one in a thousand
/// still shows.
pub fn bar_length(room: f64, value: usize, top: usize) -> f64 {
    (room * value as f64 / top.max(1) as f64).max(if value > 0 { 2.0 } else { 0.0 })
}

/// Which of `bars`, side by side over `width` points from `left`, is at `x`: the pointer
/// anywhere in a bar's column is on it. None with no bars, or outside them.
pub fn bar_under(x: f64, left: f64, width: f64, bars: usize) -> Option<usize> {
    (bars > 0 && width > 0.0 && x >= left && x < left + width).then(|| (((x - left) / width * bars as f64) as usize).min(bars - 1))
}

/// `part` of `whole` in whole per cent, rounded to the nearest; none of nothing.
pub fn share(part: usize, whole: usize) -> Option<usize> {
    (part * 100 + whole / 2).checked_div(whole)
}
