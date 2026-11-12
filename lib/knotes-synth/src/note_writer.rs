use std::fmt::Write as _;

use rand::RngExt;

use crate::Rng;
use crate::word_source::WordSource;
use crate::zipf::Zipf;

/// Where wikilinks point: popular notes ("hubs") get most links.
pub(crate) struct LinkTargets<'a> {
    pub titles: &'a [String],
    pub hubs: &'a [u32],
    pub zipf: &'a Zipf,
    pub broken_share: f64,
}

pub(crate) struct TagSet<'a> {
    pub tags: &'a [String],
    pub zipf: &'a Zipf,
}

/// Writes one synthetic note of roughly `target` bytes.
pub(crate) struct NoteWriter<'a, W: WordSource> {
    pub out: String,
    pub rng: Rng,
    pub words: &'a W,
    pub links: &'a LinkTargets<'a>,
    pub tags: &'a TagSet<'a>,
}

/// Notes below this size are a single line: no title, frontmatter or structure.
const TINY_NOTE: usize = 400;

impl<W: WordSource> NoteWriter<'_, W> {
    pub fn write(mut self, title: &str, target: usize) -> String {
        if target < TINY_NOTE {
            while self.out.len() < target {
                self.sentence(4, 12);
                self.out.push(' ');
            }
            self.out.truncate(self.out.trim_end().len());
            self.out.push('\n');
            return self.out;
        }
        if self.rng.random_bool(0.3) {
            self.frontmatter();
        }
        if self.rng.random_bool(0.5) {
            let _ = write!(self.out, "# {title}\n\n");
        }
        let mut section_start = self.out.len();
        let mut section_len = self.rng.random_range(800..4000);
        while self.out.len() < target {
            if self.out.len() - section_start > section_len {
                self.heading();
                section_start = self.out.len();
                section_len = self.rng.random_range(800..4000);
            }
            match self.rng.random::<f64>() {
                r if r < 0.70 => self.paragraph(),
                r if r < 0.83 => self.list(),
                r if r < 0.86 => self.code_block(),
                r if r < 0.88 => self.table(),
                r if r < 0.91 => self.callout(),
                r if r < 0.93 => self.math_block(),
                r if r < 0.95 => self.embed(),
                _ => self.paragraph(),
            }
        }
        self.out.truncate(self.out.trim_end().len());
        self.out.push('\n');
        self.out
    }

    fn frontmatter(&mut self) {
        self.out.push_str("---\ntags: [");
        for k in 0..self.rng.random_range(1..=4) {
            if k > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&self.tags.tags[self.tags.zipf.sample(&mut self.rng)]);
        }
        let (m, d) = (self.rng.random_range(1..=12), self.rng.random_range(1..=28));
        let _ = write!(self.out, "]\ncreated: 2026-{m:02}-{d:02}\n");
        if self.rng.random_bool(0.3) {
            self.out.push_str("aliases: [");
            self.words(2, 3, false);
            self.out.push_str("]\n");
        }
        self.out.push_str("---\n\n");
    }

    fn heading(&mut self) {
        self.out.push_str(if self.rng.random_bool(0.7) { "## " } else { "### " });
        self.words(2, 5, true);
        self.out.push_str("\n\n");
    }

    fn paragraph(&mut self) {
        for k in 0..self.rng.random_range(1..=6) {
            if k > 0 {
                self.out.push(' ');
            }
            self.sentence(5, 18);
        }
        if self.rng.random_bool(0.03) {
            let _ = write!(self.out, " ^b{:x}", self.rng.random::<u32>());
        }
        self.out.push_str("\n\n");
    }

    fn list(&mut self) {
        let style = self.rng.random_range(0..3);
        for k in 1..=self.rng.random_range(2..=7) {
            match style {
                0 => self.out.push_str("- "),
                1 => {
                    let _ = write!(self.out, "{k}. ");
                }
                _ => self.out.push_str(if self.rng.random_bool(0.5) { "- [ ] " } else { "- [x] " }),
            }
            self.sentence(3, 10);
            self.out.push('\n');
        }
        self.out.push('\n');
    }

    fn code_block(&mut self) {
        self.out.push_str("```rust\n");
        for _ in 0..self.rng.random_range(2..=10) {
            let a = self.words.content_word(&mut self.rng);
            let b = self.words.content_word(&mut self.rng);
            let _ = writeln!(self.out, "let {a} = {b}(#{});", self.rng.random_range(0..100));
        }
        self.out.push_str("```\n\n");
    }

    fn table(&mut self) {
        let cols = self.rng.random_range(2..=5);
        for row in 0..self.rng.random_range(3..=8) {
            self.out.push('|');
            for _ in 0..cols {
                self.out.push(' ');
                if row > 0 && self.rng.random_bool(0.1) {
                    self.wikilink();
                } else {
                    self.words(1, 3, false);
                }
                self.out.push_str(" |");
            }
            self.out.push('\n');
            if row == 0 {
                self.out.push('|');
                self.out.push_str(&"---|".repeat(cols));
                self.out.push('\n');
            }
        }
        self.out.push('\n');
    }

    fn callout(&mut self) {
        self.out.push_str("> [!note] ");
        self.words(2, 4, true);
        for _ in 0..self.rng.random_range(1..=3) {
            self.out.push_str("\n> ");
            self.sentence(5, 15);
        }
        self.out.push_str("\n\n");
    }

    fn math_block(&mut self) {
        const FORMULAS: [&str; 4] = [
            "\\sum_{i=1}^{n} x_i^2",
            "\\int_0^1 f(x)\\,dx",
            "f_d = \\frac{2 v f_0}{c}",
            "R = \\frac{c \\, \\Delta t}{2}",
        ];
        let f = FORMULAS[self.rng.random_range(0..FORMULAS.len())];
        let _ = write!(self.out, "$$\n{f}\n$$\n\n");
    }

    fn embed(&mut self) {
        let name = self.words.content_word(&mut self.rng);
        let _ = write!(self.out, "![[{name}-{}.png]]\n\n", self.rng.random_range(1..1000));
    }

    /// `n` words in `min..=max` separated by spaces, first one capitalized if asked.
    fn words(&mut self, min: usize, max: usize, capitalize: bool) {
        for k in 0..self.rng.random_range(min..=max) {
            if k > 0 {
                self.out.push(' ');
            }
            let w = self.words.content_word(&mut self.rng);
            push_word(&mut self.out, w, capitalize && k == 0);
        }
    }

    fn sentence(&mut self, min: usize, max: usize) {
        for k in 0..self.rng.random_range(min..=max) {
            if k > 0 {
                self.out.push(' ');
            }
            match self.rng.random::<f64>() {
                r if r < 0.012 => self.wikilink(),
                r if r < 0.016 => {
                    let tag = &self.tags.tags[self.tags.zipf.sample(&mut self.rng)];
                    let _ = write!(self.out, "#{tag}");
                }
                r if r < 0.024 => {
                    let w = self.words.word(&mut self.rng);
                    let _ = write!(self.out, "**{w}**");
                }
                r if r < 0.026 => {
                    let w = self.words.content_word(&mut self.rng);
                    let _ = write!(self.out, "[{w}](https://example.com/{})", self.rng.random_range(0..10_000));
                }
                r if r < 0.0275 => self.out.push_str("$x^2 + y^2$"),
                r if r < 0.0295 => {
                    let w = self.words.content_word(&mut self.rng);
                    let _ = write!(self.out, "`{w}()`");
                }
                _ => {
                    let w = self.words.word(&mut self.rng);
                    push_word(&mut self.out, w, k == 0);
                }
            }
        }
        self.out.push('.');
    }

    fn wikilink(&mut self) {
        self.out.push_str("[[");
        if self.rng.random_bool(self.links.broken_share) {
            self.words(2, 3, true);
        } else {
            let i = self.links.hubs[self.links.zipf.sample(&mut self.rng)] as usize;
            self.out.push_str(&self.links.titles[i]);
        }
        match self.rng.random::<f64>() {
            r if r < 0.70 => {}
            r if r < 0.85 => {
                self.out.push('|');
                self.words(1, 3, false);
            }
            _ => {
                self.out.push('#');
                self.words(2, 4, true);
            }
        }
        self.out.push_str("]]");
    }
}

fn push_word(out: &mut String, w: &str, capitalize: bool) {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) if capitalize => {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
        _ => out.push_str(w),
    }
}
