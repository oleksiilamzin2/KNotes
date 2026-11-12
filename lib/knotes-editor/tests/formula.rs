use knotes_editor::{FormulaCache, FormulaStyle, render_formula};

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()))
}

fn style(display: bool) -> FormulaStyle {
    FormulaStyle { display, size_pt: 15.0, rgb: [0, 0, 0], scale: 2.0 }
}

#[test]
fn a_formula_becomes_a_png_with_its_metrics() {
    let f = render_formula(r"\frac{a+b}{\sqrt{x^2+y^2}}", &style(true)).unwrap();
    assert!(f.width_pt > 20.0 && f.height_pt > 20.0, "{} x {}", f.width_pt, f.height_pt);
    assert!(f.baseline_pt > 0.0 && f.baseline_pt < f.height_pt, "a fraction reaches below the baseline: {}", f.baseline_pt);
    let (w, h) = png_size(&f.png);
    assert!((w as f64 - f.width_pt * 2.0).abs() <= 3.0 && (h as f64 - f.height_pt * 2.0).abs() <= 3.0, "{w}x{h} px");
    let inline = render_formula("x", &style(false)).unwrap();
    assert!(inline.height_pt < f.height_pt);
}

#[test]
fn invalid_latex_is_an_error() {
    assert!(render_formula(r"\frac{a", &style(true)).is_err());
}

#[test]
fn the_cache_renders_each_formula_once() {
    let mut cache = FormulaCache::default();
    let a = cache.get("x^2", &style(false)).unwrap();
    let b = cache.get("x^2", &style(false)).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    assert_eq!(cache.renders(), 1);
    cache.get("x^2", &style(true)).unwrap();
    assert_eq!(cache.renders(), 2, "display style is another image");
}
