//! Text attributes composed in memory before they reach the text (ADR 0012). AppKit's text
//! storage keeps attribute runs in an array: changing a range inside styled text shifts every
//! run after it, so styling a note in layers (runs, then markup, then list markers) grew with
//! the square of the note's ranges. A canvas takes the layers, one value per UTF-16 unit, and
//! hands back the final runs, which are set front to back, each once.

use std::ops::Range;

pub struct Canvas<T> {
    start: usize,
    units: Vec<T>,
}

impl<T: Copy + PartialEq> Canvas<T> {
    /// A canvas over `range` of the text, every unit `base`.
    pub fn new(range: Range<usize>, base: T) -> Self {
        Self { start: range.start, units: vec![base; range.len()] }
    }

    /// Changes every unit of `range` that lies on the canvas.
    pub fn paint(&mut self, range: Range<usize>, paint: impl Fn(&mut T)) {
        let end = self.start + self.units.len();
        let (from, to) = (range.start.clamp(self.start, end), range.end.clamp(self.start, end));
        if from < to {
            self.units[from - self.start..to - self.start].iter_mut().for_each(paint);
        }
    }

    /// The value at text position `index`, if it lies on the canvas.
    pub fn get(&self, index: usize) -> Option<&T> {
        index.checked_sub(self.start).and_then(|i| self.units.get(i))
    }

    /// Runs of equal units, in order, in text positions.
    pub fn runs(&self) -> impl Iterator<Item = (Range<usize>, T)> + '_ {
        self.runs_in(self.start..self.start + self.units.len())
    }

    /// The runs within `range` (and the canvas), cut to it.
    pub fn runs_in(&self, range: Range<usize>) -> impl Iterator<Item = (Range<usize>, T)> + '_ {
        let end = self.start + self.units.len();
        let (mut at, to) = (range.start.clamp(self.start, end) - self.start, range.end.clamp(self.start, end) - self.start);
        std::iter::from_fn(move || {
            let value = *self.units[..to].get(at)?;
            let len = self.units[at..to].iter().take_while(|&&u| u == value).count();
            let run = (self.start + at..self.start + at + len, value);
            at += len;
            Some(run)
        })
    }
}
