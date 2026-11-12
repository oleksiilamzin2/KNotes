use knotes_editor::{FormulaStyle, highlight, render_formula};

/// Every formula of the fixture note (inline and display, as written in Obsidian) typesets.
#[test]
fn every_formula_in_the_fixture_note_typesets() {
    let note = include_str!("fixtures/formulas.md");
    let formulas = highlight(note).formulas;
    assert!(formulas.len() > 80, "the fixture has many formulas: {}", formulas.len());
    let failures: Vec<String> = formulas
        .iter()
        .filter_map(|f| {
            let style = FormulaStyle { display: f.display, size_pt: 16.0, rgb: [0, 0, 0], scale: 2.0 };
            render_formula(&f.tex, &style).err().map(|e| format!("{:?}: {e}", f.tex))
        })
        .collect();
    assert!(failures.is_empty(), "{} of {} do not typeset:\n{}", failures.len(), formulas.len(), failures.join("\n"));
}

/// LaTeX that latex-rust does not typeset yet; when one starts to, move it into the fixture.
const KNOWN_GAPS: [&str; 1] = [r"\left\langle \psi \middle| \phi \right\rangle"];

#[test]
fn known_gaps_still_do_not_typeset() {
    let style = FormulaStyle { display: true, size_pt: 16.0, rgb: [0, 0, 0], scale: 2.0 };
    for tex in KNOWN_GAPS {
        assert!(render_formula(tex, &style).is_err(), "{tex} typesets now: add it to fixtures/formulas.md");
    }
}
