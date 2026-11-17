//! A card's schedule, worked out from its reviews, and the line a review is logged as.

use crate::{Fsrs, Grade, Memory};

const DAY: i64 = 86_400;
/// How soon a card graded Again comes back, in seconds.
const AGAIN: i64 = 600;

/// Where a card stands after its reviews.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schedule {
    pub memory: Memory,
    /// When it was last reviewed, and when it is due (seconds since 1970).
    pub last: i64,
    pub due: i64,
    pub reviews: u32,
    /// The times it was graded Again after it had been learned.
    pub lapses: u32,
}

/// The schedule after `reviews` (when, in seconds since 1970, and the grade), in time's order;
/// none without a review. A card graded Again comes back in ten minutes, any other after its
/// interval in days.
pub fn schedule(fsrs: &Fsrs, reviews: impl IntoIterator<Item = (i64, Grade)>) -> Option<Schedule> {
    let mut at: Option<Schedule> = None;
    for (when, grade) in reviews {
        let days = at.map_or(0.0, |s| (when - s.last).max(0) as f64 / DAY as f64);
        let memory = fsrs.review(at.map(|s| s.memory), days, grade);
        let due = if grade == Grade::Again { when + AGAIN } else { when + fsrs.interval(memory.stability) as i64 * DAY };
        at = Some(Schedule {
            memory,
            last: when,
            due,
            reviews: at.map_or(0, |s| s.reviews) + 1,
            lapses: at.map_or(0, |s| s.lapses + u32::from(grade == Grade::Again)),
        });
    }
    at
}

impl Grade {
    /// The grade a number from 1 to 4 stands for.
    pub fn from_number(n: i64) -> Option<Self> {
        [Self::Again, Self::Hard, Self::Good, Self::Easy].into_iter().find(|g| *g as i64 == n)
    }
}

/// A review as its log's line: when, the card's id and the grade, tabs between, a line break.
pub fn review_line(when: i64, card: &str, grade: Grade) -> String {
    format!("{when}\t{card}\t{}\n", grade as i32)
}

/// The review a log's line holds; none for a line that is not one (cut short, or damaged).
pub fn parse_review(line: &str) -> Option<(i64, &str, Grade)> {
    let mut parts = line.trim_end_matches(['\n', '\r']).split('\t');
    let (when, card, grade) = (parts.next()?.parse().ok()?, parts.next()?, parts.next()?.parse().ok()?);
    (parts.next().is_none() && !card.is_empty()).then_some(())?;
    Some((when, card, Grade::from_number(grade)?))
}
