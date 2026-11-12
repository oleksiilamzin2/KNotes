//! Groups top-level Markdown blocks into search blocks of bounded size.

use std::ops::Range;

use knotes_core::{Block, Span};

/// A top-level Markdown block; `heading` is set when the block is itself a heading.
pub(crate) struct TopBlock {
    pub range: Range<usize>,
    pub heading: Option<u32>,
}

/// A heading starts a new section. Inside a section, neighbouring top-level blocks are
/// merged while the result fits in `max` bytes. A single block above `max` is cut at
/// the last line break or space that fits, or at a char boundary.
pub(crate) fn chunk(source: &str, top: &[TopBlock], max: usize, out: &mut Vec<Block>) {
    let mut section = None;
    let mut current: Option<Range<usize>> = None;
    for tb in top {
        if tb.heading.is_some() {
            flush(source, current.take(), section, max, out);
            section = tb.heading;
            current = Some(tb.range.clone());
            continue;
        }
        match current.as_mut() {
            Some(c) if tb.range.end - c.start <= max => c.end = tb.range.end,
            _ => {
                flush(source, current.take(), section, max, out);
                current = Some(tb.range.clone());
            }
        }
    }
    flush(source, current, section, max, out);
}

fn flush(source: &str, range: Option<Range<usize>>, heading: Option<u32>, max: usize, out: &mut Vec<Block>) {
    let Some(range) = range else { return };
    let mut push = |r: Range<usize>| {
        if !source[r.clone()].trim().is_empty() {
            out.push(Block { span: Span::new(r.start, r.end), heading });
        }
    };
    let mut start = range.start;
    while range.end - start > max {
        let limit = source.floor_char_boundary(start + max);
        let window = &source[start..limit];
        let cut = match window.rfind('\n').or_else(|| window.rfind(' ')) {
            Some(i) if i > 0 => start + i + 1,
            _ if limit > start => limit,
            _ => source.ceil_char_boundary(start + 1),
        };
        push(start..cut);
        start = cut;
    }
    push(start..range.end);
}
