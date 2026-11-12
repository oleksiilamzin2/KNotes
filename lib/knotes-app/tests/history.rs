use knotes_app::History;

#[test]
fn back_and_forward_like_a_browser() {
    let mut h = History::default();
    assert_eq!((h.current(), h.can_back(), h.can_forward()), (None, false, false));
    h.visit("a.md");
    h.visit("b.md");
    h.visit("b.md");
    h.visit("c.md");
    assert_eq!(h.back(), Some("b.md"));
    assert_eq!(h.back(), Some("a.md"));
    assert_eq!(h.back(), None, "nothing before the first note");
    assert_eq!(h.current(), Some("a.md"));
    assert_eq!(h.forward(), Some("b.md"));
    h.visit("d.md");
    assert!(!h.can_forward(), "a new visit drops the forward stack");
    assert_eq!(h.back(), Some("b.md"));
}

#[test]
fn removed_notes_leave_the_history() {
    let mut h = History::default();
    for p in ["a.md", "gone.md", "b.md", "gone.md", "c.md"] {
        h.visit(p);
    }
    h.retain(|p| p != "gone.md");
    assert_eq!(h.back(), Some("b.md"));
    assert_eq!(h.back(), Some("a.md"));
    assert!(!h.can_back());
}

#[test]
fn depth_is_bounded() {
    let mut h = History::default();
    for i in 0..500 {
        h.visit(&format!("{i}.md"));
    }
    let mut steps = 0;
    while h.back().is_some() {
        steps += 1;
    }
    assert_eq!(steps, knotes_app::HISTORY_DEPTH);
}

#[test]
fn a_renamed_note_keeps_its_place_in_the_history() {
    let mut h = History::default();
    for p in ["a.md", "old.md", "b.md"] {
        h.visit(p);
    }
    h.back();
    h.rename("old.md", "New.md");
    assert_eq!(h.current(), Some("New.md"));
    assert_eq!(h.forward(), Some("b.md"));
    assert_eq!(h.back(), Some("New.md"));
}
