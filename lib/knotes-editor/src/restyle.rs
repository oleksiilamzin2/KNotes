//! Which part of the text needs new attributes after an edit (ADR 0012: restyle only what changed).

use std::ops::Range;

use crate::{Highlight, Style, byte_offset};

/// A replacement in UTF-16 units: `removed` units at `location` became `inserted` units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub location: usize,
    pub removed: usize,
    pub inserted: usize,
}

impl Edit {
    /// Applies the edit to a Rust copy of the text; `replacement` is the inserted text.
    pub fn apply(&self, source: &mut String, replacement: &str) {
        let start = byte_offset(source, self.location);
        let end = start + byte_offset(&source[start..], self.removed);
        source.replace_range(start..end, replacement);
    }

    /// Where a range of the text before the edit lies after it. A range starting at the edit
    /// moves past the inserted text; a range ending at the edit does not grow over it.
    pub fn move_range(&self, r: &Range<usize>) -> Range<usize> {
        let old_end = self.location + self.removed;
        let start = if r.start < self.location {
            r.start
        } else if r.start >= old_end {
            r.start - self.removed + self.inserted
        } else {
            self.location
        };
        let end = if r.end <= self.location {
            r.end
        } else if r.end > old_end {
            r.end - self.removed + self.inserted
        } else {
            self.location + self.inserted
        };
        start..end.max(start)
    }
}

/// The range (UTF-16, after the edit) whose attributes may differ: the inserted text plus
/// the runs and markup ranges between the longest common prefix and suffix of `old` (moved by
/// the edit) and `new`. So a typed fence covers all text below it, and a removed setext
/// underline the line above. Linear in the number of ranges, with no allocation.
pub fn changed_region(old: &Highlight, new: &Highlight, edit: Edit) -> (usize, usize) {
    let mut region = (edit.location, edit.location + edit.inserted);
    widen_by_difference(
        (old.runs.len(), |i: usize| (edit.move_range(&old.runs[i].range), Some(old.runs[i].style))),
        (new.runs.len(), |i: usize| (new.runs[i].range.clone(), Some(new.runs[i].style))),
        &mut region,
    );
    widen_by_difference(
        (old.markup.len(), |i: usize| (edit.move_range(&old.markup[i]), None)),
        (new.markup.len(), |i: usize| (new.markup[i].clone(), None)),
        &mut region,
    );
    region
}

type Key = (Range<usize>, Option<Style>);

fn widen_by_difference(old: (usize, impl Fn(usize) -> Key), new: (usize, impl Fn(usize) -> Key), region: &mut (usize, usize)) {
    let ((n_old, old), (n_new, new)) = (old, new);
    let common = n_old.min(n_new);
    let prefix = (0..common).take_while(|&i| old(i) == new(i)).count();
    let suffix = (0..common - prefix).take_while(|&i| old(n_old - 1 - i) == new(n_new - 1 - i)).count();
    for (r, _) in (prefix..n_old - suffix).map(&old).chain((prefix..n_new - suffix).map(&new)) {
        *region = (region.0.min(r.start), region.1.max(r.end));
    }
}
