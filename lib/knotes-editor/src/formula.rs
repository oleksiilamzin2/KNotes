//! LaTeX formulas typeset to images for the live preview (ADR 0017), with `latex-rust`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use latex_rust::{Color, Dim, MathFont, MathStyle, PngBackground, PngOptions, layout_with_em_size_pt, parse, render_png};

/// How to typeset a formula.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaStyle {
    /// Display (`$$…$$`) or inline (`$…$`) style.
    pub display: bool,
    /// Em size in points.
    pub size_pt: f64,
    /// Glyph colour, sRGB.
    pub rgb: [u8; 3],
    /// Pixels per point (2 on Retina screens).
    pub scale: f64,
}

/// A typeset formula: PNG pixels and its size in points; `baseline_pt` is the distance from
/// the top to the mathematical baseline, which sits on the text baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaImage {
    pub png: Vec<u8>,
    pub width_pt: f64,
    pub height_pt: f64,
    pub baseline_pt: f64,
}

fn font() -> Result<&'static MathFont, String> {
    static FONT: OnceLock<Result<MathFont, String>> = OnceLock::new();
    FONT.get_or_init(|| MathFont::stix_two_math().map_err(|e| e.to_string())).as_ref().map_err(Clone::clone)
}

fn to_f64(d: &Dim) -> f64 {
    d.as_ratio().map_or(0.0, |(n, d)| n as f64 / d as f64)
}

/// Typesets `tex` (LaTeX math, without dollars); the error says what is wrong with it.
pub fn render_formula(tex: &str, style: &FormulaStyle) -> Result<FormulaImage, String> {
    let font = font()?;
    let ast = parse(tex).map_err(|e| e.to_string())?;
    let math_style = if style.display { MathStyle::Display } else { MathStyle::Text };
    // Exact rationals in latex-rust: sizes go in as hundredths of a point.
    let em = Dim::ratio((style.size_pt * 100.0).round() as i64, 100);
    let tree = layout_with_em_size_pt(&ast, font, math_style, &em).map_err(|e| e.to_string())?;
    let options = PngOptions {
        font_size_pt: em.clone(),
        dpi: Dim::ratio((72.0 * style.scale * 100.0).round() as i64, 100),
        color: Color::rgb(style.rgb[0], style.rgb[1], style.rgb[2]),
        background: PngBackground::Transparent,
        display: style.display,
    };
    let png = render_png(&tree, font, &options).map_err(|e| e.to_string())?;
    let em_pt = style.size_pt;
    Ok(FormulaImage {
        png,
        width_pt: to_f64(&tree.width) * em_pt,
        height_pt: (to_f64(&tree.height) + to_f64(&tree.depth)) * em_pt,
        baseline_pt: to_f64(&tree.height) * em_pt,
    })
}

/// Source, display, size, colour and scale of a typeset formula.
type FormulaKey = (String, bool, u64, [u8; 3], u64);

/// Typeset formulas by source and style: a note's formulas are typeset once, not on every
/// restyle.
#[derive(Default)]
pub struct FormulaCache {
    images: HashMap<FormulaKey, Result<Arc<FormulaImage>, String>>,
    renders: usize,
}

impl FormulaCache {
    pub fn get(&mut self, tex: &str, style: &FormulaStyle) -> Result<Arc<FormulaImage>, String> {
        let key = (tex.to_owned(), style.display, style.size_pt.to_bits(), style.rgb, style.scale.to_bits());
        if let Some(image) = self.images.get(&key) {
            return image.clone();
        }
        self.renders += 1;
        let image = render_formula(tex, style).map(Arc::new);
        self.images.insert(key, image.clone());
        image
    }

    /// Formulas typeset so far (for tests).
    pub fn renders(&self) -> usize {
        self.renders
    }
}
