//! A card's schedule, worked out from its reviews, and the line a review is logged as.
//!
//! A new card is learned in steps before its days begin, and a forgotten one learned again, as
//! the reference schedules them (py-fsrs 4.1.2): Again goes to the first step, Hard stays at
//! its step, Good to the next (past the last, to its days), Easy to its days at once.

use crate::{Fsrs, Grade, Memory};

const DAY: i64 = 86_400;

/// The steps a card is learned in, in seconds: a new one's, and a forgotten one's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steps {
    pub learning: Vec<i64>,
    pub relearning: Vec<i64>,
}

impl Default for Steps {
    /// The reference's: a minute and ten minutes for a new card, ten minutes for a forgotten one.
    fn default() -> Self {
        Self { learning: vec![60, 600], relearning: vec![600] }
    }
}

impl Steps {
    /// A new card learned in `minutes`' steps (none: its days begin at its first review); a
    /// forgotten one comes back in ten minutes.
    pub fn of_minutes(minutes: &[u32]) -> Self {
        Self { learning: minutes.iter().map(|m| i64::from(*m) * 60).collect(), relearning: vec![600] }
    }
}

/// Where a card stands after its reviews.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schedule {
    pub memory: Memory,
    /// When it was first reviewed, last reviewed, and when it is due (seconds since 1970).
    pub first: i64,
    pub last: i64,
    pub due: i64,
    pub reviews: u32,
    /// The times it was graded Again after its first review.
    pub lapses: u32,
    /// The step it waits at while it is learned (`relearning`: after it was forgotten); none
    /// once its days have begun.
    pub step: Option<u8>,
    pub relearning: bool,
}

impl Schedule {
    /// A card as it stands with `memory`, last reviewed at `last`, waiting at `step`: what a
    /// review of it starts from (its counts and its due day play no part in the next).
    pub fn standing(memory: Memory, last: i64, step: Option<u8>, relearning: bool) -> Self {
        Self { memory, first: last, last, due: last, reviews: 1, lapses: 0, step, relearning }
    }
}

/// Where a step's grade leads: the step to wait at and for how long (seconds), or none when
/// the card's days begin. `at`: the step it waits at now.
fn walk(steps: &[i64], at: usize, grade: Grade) -> Option<(usize, i64)> {
    // No steps, or more were set when it was scheduled: its days begin.
    let now = *steps.get(at)?;
    match grade {
        Grade::Again => Some((0, steps[0])),
        Grade::Hard => Some((at, if at > 0 { now } else if steps.len() == 1 { now * 3 / 2 } else { (now + steps[1]) / 2 })),
        Grade::Good => steps.get(at + 1).map(|next| (at + 1, *next)),
        Grade::Easy => None,
    }
}

/// The schedule after a review graded `grade` at `when` (seconds since 1970) of a card that
/// stood at `before` (none: a new one).
pub fn review(fsrs: &Fsrs, steps: &Steps, before: Option<&Schedule>, when: i64, grade: Grade) -> Schedule {
    let days = before.map_or(0.0, |s| (when - s.last).max(0) as f64 / DAY as f64);
    let memory = fsrs.review(before.map(|s| s.memory), days, grade);
    let learned = match before {
        // A new card is at its first step.
        None => walk(&steps.learning, 0, grade).map(|to| (to, false)),
        Some(Schedule { step: Some(at), relearning, .. }) => walk(if *relearning { &steps.relearning } else { &steps.learning }, usize::from(*at), grade).map(|to| (to, *relearning)),
        // Forgotten: learned again from the first step.
        Some(_) if grade == Grade::Again => steps.relearning.first().map(|wait| ((0, *wait), true)),
        Some(_) => None,
    };
    let (due, step, relearning) = match learned {
        Some(((step, wait), relearning)) => (when + wait, Some(step.min(usize::from(u8::MAX)) as u8), relearning),
        None => (when + fsrs.interval(memory.stability) as i64 * DAY, None, false),
    };
    Schedule {
        memory,
        first: before.map_or(when, |s| s.first),
        last: when,
        due,
        reviews: before.map_or(0, |s| s.reviews) + 1,
        lapses: before.map_or(0, |s| s.lapses + u32::from(grade == Grade::Again)),
        step,
        relearning,
    }
}

/// The schedule after `reviews` (when, in seconds since 1970, and the grade), in time's order;
/// none without a review.
pub fn schedule(fsrs: &Fsrs, steps: &Steps, reviews: impl IntoIterator<Item = (i64, Grade)>) -> Option<Schedule> {
    let mut at: Option<Schedule> = None;
    for (when, grade) in reviews {
        at = Some(review(fsrs, steps, at.as_ref(), when, grade));
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

/// What a log's line says: a review, or a review taken back (the user graded by mistake and
/// undid it). A log is only ever added to, so a review is taken back by a line after it: the
/// same time and card, the grade 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logged<'a> {
    Review(i64, &'a str, Grade),
    /// The review of this card at this time is as if it had not been.
    TakenBack(i64, &'a str),
    /// The card was paused at this time (asked no more until it is resumed), or resumed.
    Paused(i64, &'a str, bool),
}

/// The line that pauses `card` at `when` (`P`), or resumes it (`R`). Of a card's such lines,
/// in whatever logs, the latest by its time says how the card stands.
pub fn paused_line(when: i64, card: &str, paused: bool) -> String {
    format!("{when}\t{card}\t{}\n", if paused { 'P' } else { 'R' })
}

/// The line that takes back the review of `card` logged at `when`.
pub fn taken_back_line(when: i64, card: &str) -> String {
    format!("{when}\t{card}\t0\n")
}

/// What a log's line says; none for a line that says nothing (cut short, or damaged).
pub fn parse_logged(line: &str) -> Option<Logged<'_>> {
    if let Some((when, card, grade)) = parse_review(line) {
        return Some(Logged::Review(when, card, grade));
    }
    let mut parts = line.trim_end_matches(['\n', '\r']).split('\t');
    let (when, card, grade) = (parts.next()?.parse().ok()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && !card.is_empty()).then_some(())?;
    match grade {
        "0" => Some(Logged::TakenBack(when, card)),
        "P" => Some(Logged::Paused(when, card, true)),
        "R" => Some(Logged::Paused(when, card, false)),
        _ => None,
    }
}
