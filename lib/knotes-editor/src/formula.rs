//! LaTeX formulas typeset to images for the live preview (ADR 0017), with RaTeX (KaTeX's
//! layout and fonts in Rust).

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, GlyphId};
use ratex_font::FontId;
use ratex_font_loader::SystemFontResolver;
use ratex_layout::{FallbackGlyphMetrics, LayoutOptions, layout_with_fallback_metrics, to_display_list, validate_layout};
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::color::Color;
use ratex_types::math_style::MathStyle;

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

/// Typesets `tex` (LaTeX math, without dollars); the error says what is wrong with it.
pub fn render_formula(tex: &str, style: &FormulaStyle) -> Result<FormulaImage, String> {
    let ast = ratex_parser::parser::parse(tex).map_err(|e| e.to_string())?;
    let [r, g, b] = style.rgb.map(|c| f32::from(c) / 255.0);
    let math_style = if style.display { MathStyle::Display } else { MathStyle::Text };
    let options = LayoutOptions::default().with_style(math_style).with_color(Color::rgb(r, g, b));
    let tree = layout_with_fallback_metrics(&ast, &options, Arc::new(fallback_metrics(tex)));
    validate_layout(&tree)?;
    let list = to_display_list(&tree);
    let render = RenderOptions {
        font_size: style.size_pt as f32,
        padding: 0.0,
        background_color: Color::new(0.0, 0.0, 0.0, 0.0),
        font_dir: String::new(),
        device_pixel_ratio: style.scale as f32,
    };
    let png = render_to_png(&list, &render)?;
    // The display list measures in ems.
    let em = style.size_pt;
    Ok(FormulaImage { png, width_pt: list.width * em, height_pt: (list.height + list.depth) * em, baseline_pt: list.height * em })
}

/// Ems of each letter of `tex` the KaTeX fonts lack (Cyrillic in `\text{…}`, say), measured
/// in the system font RaTeX draws it with; without them every such letter is half an em wide.
fn fallback_metrics(tex: &str) -> HashMap<char, FallbackGlyphMetrics> {
    let mut metrics = HashMap::new();
    let mut missing: Vec<char> = tex.chars().filter(|&c| !c.is_ascii() && ratex_font::get_char_metrics(FontId::MainRegular, c as u32).is_none()).collect();
    if missing.is_empty() {
        return metrics;
    }
    missing.sort_unstable();
    missing.dedup();
    // The fonts RaTeX falls back to, in its order.
    let fonts = SystemFontResolver::new();
    let fonts: Vec<_> = [FontId::CjkRegular, FontId::CjkFallback].into_iter().filter_map(|id| fonts.get(id).ok().flatten().map(|f| f.font().clone())).collect();
    for c in missing {
        let found = fonts.iter().find_map(|font| {
            let glyph = font.glyph_id(c);
            (glyph != GlyphId(0)).then_some((font, glyph))
        });
        let Some((font, glyph)) = found else { continue };
        let em = f64::from(font.units_per_em().unwrap_or(1000.0));
        // Unscaled bounds run from the top (`min.y`) to the bottom (`max.y`), y up.
        let (top, bottom) = font.outline(glyph).map_or((0.0, 0.0), |o| (f64::from(o.bounds.min.y), f64::from(o.bounds.max.y)));
        metrics.insert(c, FallbackGlyphMetrics { width: f64::from(font.h_advance_unscaled(glyph)) / em, height: top.max(0.0) / em, depth: (-bottom).max(0.0) / em });
    }
    metrics
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
