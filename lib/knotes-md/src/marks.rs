//! Obsidian highlights: `==text==` within a line.

use std::ops::Range;

/// The highlights of `source`, their signs included, outside the `skip` ranges (code,
/// formulas, comments; in any order, one inside another too): a sign before a non-space opens,
/// the next after one closes; three or more `=` are no sign (a heading's underline).
pub fn mark_ranges(source: &str, skip: &[Range<usize>]) -> Vec<Range<usize>> {
    // Merged, so the last starting before a place is the one that can hold it.
    let mut sorted = skip.to_vec();
    sorted.sort_by_key(|r| r.start);
    let mut skip: Vec<Range<usize>> = Vec::with_capacity(sorted.len());
    for r in sorted {
        match skip.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => skip.push(r),
        }
    }
    let skipped = |at: usize| {
        let after = skip.partition_point(|r| r.start <= at);
        after > 0 && at < skip[after - 1].end
    };
    let mut marks = Vec::new();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        let bytes = line.as_bytes();
        let (mut i, mut open) = (0, None);
        while i + 1 < bytes.len() {
            if !(bytes[i] == b'=' && bytes[i + 1] == b'=') {
                i += 1;
                continue;
            }
            let signs = bytes[i..].iter().take_while(|&&b| b == b'=').count();
            if signs == 2 && !skipped(line_start + i) {
                let at = line_start + i;
                match open {
                    None if line[i + 2..].chars().next().is_some_and(|c| !c.is_whitespace()) => open = Some(at),
                    Some(start) if line[..i].chars().next_back().is_some_and(|c| !c.is_whitespace()) => {
                        marks.push(start..at + 2);
                        open = None;
                    }
                    _ => {}
                }
            }
            i += signs;
        }
        line_start += line.len();
    }
    marks
}
