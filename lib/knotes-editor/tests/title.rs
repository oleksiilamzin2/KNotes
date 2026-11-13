use knotes_editor::replace_title;

#[test]
fn the_title_takes_the_new_name_keeping_its_heading_markers() {
    let is_old = |t: &str| t == "Old name";
    assert_eq!(replace_title("# Old name\n\nbody", "New", is_old).as_deref(), Some("# New\n\nbody"));
    assert_eq!(replace_title("Old name\nsame paragraph", "New", is_old).as_deref(), Some("New\nsame paragraph"));
    assert_eq!(replace_title("---\na: 1\n---\n## Old name  \n", "New", is_old).as_deref(), Some("---\na: 1\n---\n## New  \n"));
}

#[test]
fn a_title_that_was_not_the_old_name_stays() {
    assert_eq!(replace_title("A paragraph that starts the note\n", "New", |t| t == "Old"), None);
    assert_eq!(replace_title("- list first\n", "New", |_| true), None);
}
