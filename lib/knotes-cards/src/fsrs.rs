//! FSRS-5, the scheduler (Ye, 2024): a card's memory is its stability (the days until it is
//! recalled with a chance of 90%) and its difficulty (1 to 10); a review grades the recall,
//! and the memory after it follows from the memory before, the days since, and the grade.

/// How well a card was recalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

/// What is remembered of a card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Memory {
    /// Days until the chance of recalling it falls to 90%.
    pub stability: f64,
    /// 1 (easy) to 10 (hard).
    pub difficulty: f64,
}

/// The scheduler: its 19 weights, and the chance of recall cards come back at.
#[derive(Debug, Clone, PartialEq)]
pub struct Fsrs {
    pub weights: [f64; 19],
    /// The chance of recall a card is due at (0.9: when it has fallen to 90%).
    pub retention: f64,
}

/// The forgetting curve's shape: R(t) = (1 + FACTOR·t/S)^DECAY, 0.9 at t = S.
const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;
const STABILITY_MIN: f64 = 0.001;
/// The longest interval, in days (a century).
const INTERVAL_MAX: f64 = 36_500.0;

impl Default for Fsrs {
    /// FSRS-5's default weights, fitted on many users' reviews; cards due at 90%.
    fn default() -> Self {
        Self {
            weights: [0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621],
            retention: 0.9,
        }
    }
}

impl Fsrs {
    /// The chance of recalling a card of `stability` `days` after its last review.
    pub fn recall(&self, stability: f64, days: f64) -> f64 {
        (1.0 + FACTOR * days.max(0.0) / stability.max(STABILITY_MIN)).powf(DECAY)
    }

    /// The days until a card of `stability` is due: at least one, whole.
    pub fn interval(&self, stability: f64) -> f64 {
        let days = stability / FACTOR * (self.retention.powf(1.0 / DECAY) - 1.0);
        days.round().clamp(1.0, INTERVAL_MAX)
    }

    /// The memory after a review graded `grade`, `days` after the last one; `before` is none at
    /// a card's first review.
    pub fn review(&self, before: Option<Memory>, days: f64, grade: Grade) -> Memory {
        let w = &self.weights;
        let g = grade as i32 as f64;
        let Some(Memory { stability, difficulty }) = before else {
            return Memory { stability: w[grade as usize - 1].max(STABILITY_MIN), difficulty: self.first_difficulty(g) };
        };
        let recall = self.recall(stability, days);
        let after = if days < 1.0 {
            // The same day again: a short step, no forgetting to measure.
            stability * (w[17] * (g - 3.0 + w[18])).exp()
        } else if grade == Grade::Again {
            let forgotten = w[11] * difficulty.powf(-w[12]) * ((stability + 1.0).powf(w[13]) - 1.0) * (w[14] * (1.0 - recall)).exp();
            forgotten.min(stability)
        } else {
            let hard = if grade == Grade::Hard { w[15] } else { 1.0 };
            let easy = if grade == Grade::Easy { w[16] } else { 1.0 };
            stability * (1.0 + w[8].exp() * (11.0 - difficulty) * stability.powf(-w[9]) * ((w[10] * (1.0 - recall)).exp() - 1.0) * hard * easy)
        };
        // Harder after a poor recall, easier after an easy one, drawn back towards an easy card's.
        let moved = difficulty - w[6] * (g - 3.0) * (10.0 - difficulty) / 9.0;
        let difficulty = (w[7] * self.first_difficulty(4.0) + (1.0 - w[7]) * moved).clamp(1.0, 10.0);
        Memory { stability: after.max(STABILITY_MIN), difficulty }
    }

    fn first_difficulty(&self, grade: f64) -> f64 {
        (self.weights[4] - (self.weights[5] * (grade - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
    }
}
