//! Formulas (ADR 0017): typeset, drawn in the room their hidden source keeps.

use super::*;

impl LivePreview {
    /// `light` or `dark`: the colour formulas are typeset in (probe mode).
    pub fn formula_ink(&self) -> &'static str {
        if self.ivars().dark.get() { "dark" } else { "light" }
    }

    /// Text colour for typeset formulas, and the typeset image of `f` (cached).
    pub(super) fn formula_image(&self, f: &FormulaRun) -> Result<Arc<FormulaImage>, String> {
        let rgb = if self.ivars().dark.get() { [235, 235, 235] } else { [29, 29, 31] };
        let style = FormulaStyle { display: f.display, size_pt: BODY_SIZE + 1.0, rgb, scale: 2.0 };
        self.ivars().formula_cache.borrow_mut().get(&f.tex, &style)
    }

    /// A formula outside the cursor's paragraph: its source hidden and its room reserved
    /// (width by kerning, height by the line height) for the typeset image. With the cursor
    /// in it: its source, and room below for a display formula's preview. A formula that
    /// does not typeset shows its source marked as an error.
    pub(super) fn paint_formula<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, f: &FormulaRun, revealed: bool) {
        let look = &self.ivars().look;
        let paragraphs = paragraph(storage, ns(&f.range));
        let block = f.display && is_block(storage, f);
        if block {
            // A display formula owns its lines: their paragraph styles are its own.
            canvas.paint(paragraphs.0..paragraphs.1, |p| p.para = 0);
        }
        let source = |canvas: &mut Canvas<Paint>, id: u16| {
            canvas.paint(f.range.clone(), |p| {
                p.look = id;
                p.kern = 0;
            })
        };
        let image = match self.formula_image(f) {
            Ok(image) => image,
            Err(_) => return source(canvas, palette.look(&look.mono, &look.error, "math-error")),
        };
        let reserved = image.height_pt + 2.0 * PREVIEW_GAP;
        if revealed {
            source(canvas, palette.look(&look.mono, &look.text, "math"));
            if block {
                // Room for the preview under the source's last line.
                let last = paragraph(storage, NSRange::new(f.range.end.saturating_sub(1), 0));
                let para = palette.para(Para { spacing_after: reserved, ..Para::default() });
                canvas.paint(last.0..last.1, |p| p.para = para);
            }
            return;
        }
        source(canvas, palette.look(&look.hidden, &look.clear, "formula"));
        if block {
            // One centred line as tall as the image; the source's other lines fold away.
            let first = paragraph(storage, NSRange::new(f.range.start, 0));
            let line = palette.para(Para { centred: true, min_line: reserved, max_line: reserved, ..Para::default() });
            canvas.paint(first.0..first.1, |p| p.para = line);
            let folded = palette.para(Para { max_line: 0.01, ..Para::default() });
            canvas.paint(first.1..paragraphs.1, |p| p.para = folded);
        }
        // On the first character: the source has spaces where lines may break, and the reserved
        // width must stay with the image (on the last character a break inside the source
        // carried it to the next line). A formula too wide for the rest of a line now moves to
        // the next one whole.
        let kern = palette.kern(image.width_pt);
        canvas.paint(f.range.start..f.range.start + 1, |p| p.kern = kern);
    }

    /// Width of `range` of `text` as `canvas` will show it: each run in its font (measured once
    /// for its text), with its kerning.
    pub(super) fn shown_width(&self, canvas: &Canvas<Paint>, palette: &Palette, text: &NSString, range: &Range<usize>) -> f64 {
        let mut cache = self.ivars().look.piece_widths.borrow_mut();
        if cache.len() > MAX_PIECE_WIDTHS {
            cache.clear();
        }
        canvas
            .runs_in(range.clone())
            .map(|(run, paint)| {
                let piece = text.substringWithRange(ns(&run));
                let font = palette.font_of(paint.look);
                let key = (piece.to_string(), font as *const NSFont as usize);
                let width = *cache.entry(key).or_insert_with(|| text_width(&piece, font));
                width + palette.kern_of(paint.kern) * run.len() as f64
            })
            .sum()
    }

    /// Line height of each paragraph in `region` with inline formulas: enough for the tallest
    /// typeset one above and below the baseline. Paragraphs of display formulas are left to
    /// them; the paragraph's other settings (a list item's indentation) stay.
    pub(super) fn paint_formula_lines(
        &self,
        canvas: &mut Canvas<Paint>,
        palette: &mut Palette,
        storage: &NSTextStorage,
        styled: &Highlight,
        region: (usize, usize),
    ) {
        let font = &self.ivars().look.body;
        let (ascent, descent) = (font.ascender(), -font.descender());
        let mut paragraphs: Vec<(usize, usize)> =
            styled.formulas.iter().filter(|f| f.range.start < region.1 && f.range.end > region.0).map(|f| paragraph(storage, ns(&f.range))).collect();
        paragraphs.dedup();
        for para in paragraphs {
            let in_paragraph = |f: &&FormulaRun| f.range.start < para.1 && f.range.end > para.0;
            if styled.formulas.iter().filter(in_paragraph).any(|f| f.display && is_block(storage, f)) {
                continue;
            }
            let (mut above, mut below) = (ascent, descent);
            for f in styled.formulas.iter().filter(in_paragraph).filter(|f| !self.formula_shown(f)) {
                if let Ok(image) = self.formula_image(f) {
                    above = above.max(image.baseline_pt);
                    below = below.max(image.height_pt - image.baseline_pt);
                }
            }
            let base = palette.para_of(canvas.get(para.0).map_or(0, |p| p.para));
            let min_line = above + below + 2.0;
            if (above > ascent || below > descent) && min_line > base.min_line {
                let id = palette.para(Para { min_line, ..base });
                canvas.paint(para.0..para.1, |p| p.para = id);
            }
        }
    }
}

/// Whether a display formula fills its paragraphs (its own lines) rather than sitting in text.
pub(super) fn is_block(storage: &NSTextStorage, f: &FormulaRun) -> bool {
    let (start, end) = paragraph(storage, ns(&f.range));
    let text = storage.string();
    let trailing = (f.range.end..end).all(|i| matches!(text.characterAtIndex(i), 0x0a | 0x0d | 0x20));
    start == f.range.start && trailing
}
