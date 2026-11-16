//! Obsidian comments: `%%…%%`, inline or across lines. They are not the note's tags, links,
//! anchors or headings; one left open runs to the end of the note, as in Obsidian.

use std::ops::Range;

/// The comments of `source`, their `%%` included, outside the `verbatim` ranges (code,
/// formulas, frontmatter; sorted by start, apart).
pub fn comment_ranges(source: &str, verbatim: &[Range<usize>]) -> Vec<Range<usize>> {
    let in_verbatim = |at: usize| {
        let after = verbatim.partition_point(|r| r.start <= at);
        after > 0 && at < verbatim[after - 1].end
    };
    let mut comments = Vec::new();
    let mut open = None;
    let mut from = 0;
    while let Some(found) = source[from..].find("%%") {
        let at = from + found;
        from = at + 2;
        if in_verbatim(at) {
            continue;
        }
        match open.take() {
            None => open = Some(at),
            Some(start) => comments.push(start..at + 2),
        }
    }
    if let Some(start) = open {
        comments.push(start..source.len());
    }
    comments
}
