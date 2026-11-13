//! A PDF page's text and where its characters are, for selecting and copying it in the viewer
//! (ADR 0022). Places are in the page as shown: x and y from its top left corner, as
//! fractions of its width and height.

use std::ops::Range;

/// A box on a page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    /// This box in PDF space (y up) placed on the page as shown: `crop` is the page's crop
    /// box, `rotation` the degrees it is turned clockwise when shown.
    pub fn shown(self, crop: Bounds, rotation: i64) -> Bounds {
        // Fractions of the crop box, y down.
        let (left, right) = ((self.x - crop.x) / crop.width, (self.right() - crop.x) / crop.width);
        let (top, bottom) = (1.0 - (self.bottom() - crop.y) / crop.height, 1.0 - (self.y - crop.y) / crop.height);
        let (x0, x1, y0, y1) = match rotation.rem_euclid(360) {
            90 => (1.0 - bottom, 1.0 - top, left, right),
            180 => (1.0 - right, 1.0 - left, 1.0 - bottom, 1.0 - top),
            270 => (top, bottom, 1.0 - right, 1.0 - left),
            _ => (left, right, top, bottom),
        };
        Bounds { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
    }

    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// A character with no box has no place on the page.
    fn is_placed(&self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }

    fn union(self, other: Bounds) -> Bounds {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Bounds { x, y, width: self.right().max(other.right()) - x, height: self.bottom().max(other.bottom()) - y }
    }
}

/// A line of the page's text: its characters, without the break after them, and the box
/// around them.
struct Line {
    range: Range<usize>,
    bounds: Bounds,
}

/// A page's characters, in its text's order, and their boxes.
pub struct PageText {
    chars: Vec<char>,
    bounds: Vec<Bounds>,
    lines: Vec<Line>,
}

impl PageText {
    /// From the page's characters and their boxes; line breaks end lines.
    pub fn new(glyphs: impl IntoIterator<Item = (char, Bounds)>) -> Self {
        let (chars, bounds): (Vec<char>, Vec<Bounds>) = glyphs.into_iter().unzip();
        let mut lines = Vec::new();
        let mut start = 0;
        for end in (0..=chars.len()).filter(|&i| i == chars.len() || is_break(chars[i])) {
            let placed = bounds[start..end].iter().filter(|b| b.is_placed()).copied().reduce(Bounds::union);
            if let Some(bounds) = placed {
                lines.push(Line { range: start..end, bounds });
            }
            start = end + 1;
        }
        Self { chars, bounds, lines }
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Where the caret goes for a click at `x`, `y`: before the first character of the line
    /// there (or the nearest) whose middle is right of the point, or at the line's end.
    pub fn caret_at(&self, x: f64, y: f64) -> usize {
        let Some(line) = self.line_near(x, y) else { return 0 };
        line.range.clone().find(|&i| self.bounds[i].is_placed() && self.bounds[i].x + self.bounds[i].width / 2.0 > x).unwrap_or(line.range.end)
    }

    /// The word at `x`, `y`: the letters and digits around the character there (or the
    /// nearest on its line); any other character alone.
    pub fn word_at(&self, x: f64, y: f64) -> Range<usize> {
        let Some(line) = self.line_near(x, y) else { return 0..0 };
        let distance = |i: usize| {
            let b = self.bounds[i];
            if !b.is_placed() { f64::INFINITY } else { (b.x - x).max(x - b.right()).max(0.0) }
        };
        let Some(hit) = line.range.clone().min_by(|&a, &b| distance(a).total_cmp(&distance(b))) else { return line.range.clone() };
        if !is_word(self.chars[hit]) {
            return hit..hit + 1;
        }
        let start = (line.range.start..hit).rev().find(|&i| !is_word(self.chars[i])).map_or(line.range.start, |i| i + 1);
        let end = (hit..line.range.end).find(|&i| !is_word(self.chars[i])).unwrap_or(line.range.end);
        start..end
    }

    /// The line at `x`, `y` (or the nearest), without its break.
    pub fn line_at(&self, x: f64, y: f64) -> Range<usize> {
        self.line_near(x, y).map_or(0..0, |line| line.range.clone())
    }

    /// The text of `range`, line breaks kept.
    pub fn text(&self, range: Range<usize>) -> String {
        self.chars[range.start.min(self.len())..range.end.min(self.len())].iter().collect()
    }

    /// Boxes covering `range`, one per line it takes in, as tall as the line.
    pub fn highlight(&self, range: Range<usize>) -> Vec<Bounds> {
        self.lines
            .iter()
            .filter_map(|line| {
                let part = range.start.max(line.range.start)..range.end.min(line.range.end);
                if part.is_empty() {
                    return None;
                }
                let placed = self.bounds[part].iter().filter(|b| b.is_placed()).copied().reduce(Bounds::union)?;
                Some(Bounds { x: placed.x, y: line.bounds.y, width: placed.width, height: line.bounds.height })
            })
            .collect()
    }

    /// The line `x`, `y` is in, or the nearest: by height first, then sideways (lines side by
    /// side, as in columns).
    fn line_near(&self, x: f64, y: f64) -> Option<&Line> {
        let away = |from: f64, to: f64, v: f64| (from - v).max(v - to).max(0.0);
        self.lines.iter().min_by(|a, b| {
            let key = |l: &Line| (away(l.bounds.y, l.bounds.bottom(), y), away(l.bounds.x, l.bounds.right(), x));
            let (ka, kb) = (key(a), key(b));
            ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        })
    }
}

fn is_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
