use knotes_core::{LinkKind, LinkResolver};
use knotes_index::LinkTargets;

fn targets(paths: &[&str]) -> LinkTargets {
    LinkTargets::new(paths.iter().copied())
}

#[test]
fn bare_names_pick_the_shortest_path_ignoring_case_and_normalization() {
    let t = targets(&["A/Note.md", "Note.md", "B/И\u{0306}од.md"]);
    assert_eq!(t.resolve(LinkKind::Wiki, "note", "A/x.md").as_deref(), Some("Note.md"));
    assert_eq!(t.resolve(LinkKind::Wiki, "Йод", "x.md").as_deref(), Some("B/И\u{0306}од.md"));
    assert_eq!(t.resolve(LinkKind::Wiki, "Missing", "x.md"), None);
}

#[test]
fn links_with_a_folder_need_that_path_or_a_path_ending_with_it() {
    let t = targets(&["x/A/Note.md", "Note.md"]);
    assert_eq!(t.resolve(LinkKind::Wiki, "A/Note", "y.md").as_deref(), Some("x/A/Note.md"));
    assert_eq!(t.resolve(LinkKind::Wiki, "B/Note", "y.md"), None);
}

#[test]
fn markdown_links_try_the_linking_note_folder_first() {
    let t = targets(&["A/Note.md", "Note.md", "att/img.png"]);
    assert_eq!(t.resolve(LinkKind::Markdown, "Note.md", "A/x.md").as_deref(), Some("A/Note.md"));
    assert_eq!(t.resolve(LinkKind::Markdown, "../Note.md", "A/x.md").as_deref(), Some("Note.md"));
    assert_eq!(t.resolve(LinkKind::Wiki, "img.png", "x.md").as_deref(), Some("att/img.png"));
}

#[test]
fn links_inside_the_note_resolve_to_it_and_external_ones_never() {
    let t = targets(&["n.md"]);
    assert_eq!(t.resolve(LinkKind::Wiki, "", "n.md").as_deref(), Some("n.md"));
    assert_eq!(t.resolve(LinkKind::External, "https://example.com", "n.md"), None);
}

#[test]
fn link_text_is_the_bare_name_when_it_resolves_to_the_file() {
    let t = targets(&["A/Note.md", "Note.md", "B/Unique.md", "att/img.png"]);
    assert_eq!(t.link_text("B/Unique.md", "x.md"), "Unique");
    assert_eq!(t.link_text("Note.md", "A/x.md"), "Note");
    assert_eq!(t.link_text("A/Note.md", "A/x.md"), "A/Note", "the bare name would open Note.md");
    assert_eq!(t.link_text("att/img.png", "x.md"), "img.png");
}
