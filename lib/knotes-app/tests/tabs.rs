use knotes_app::{AfterClose, TabPlace, Tabs};

fn paths(tabs: &Tabs) -> Vec<&str> {
    tabs.paths().collect()
}

/// Tabs of `paths` opened beside one another, the last shown.
fn tabs_of(paths: &[&str]) -> Tabs {
    let mut tabs = Tabs::default();
    for p in paths {
        tabs.open_beside(p);
    }
    tabs
}

#[test]
fn a_document_opens_in_the_tab_shown_and_the_first_one_makes_a_tab() {
    let mut tabs = Tabs::default();
    assert_eq!(tabs.navigate("a.md"), None);
    tabs.navigate("b.md");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["b.md"], Some("b.md")));
}

#[test]
fn a_document_opened_beside_gets_a_tab_after_the_one_shown_and_is_shown() {
    let mut tabs = tabs_of(&["a.md", "b.md"]);
    tabs.navigate("a.md");
    tabs.open_beside("doc.pdf");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["a.md", "doc.pdf", "b.md"], Some("doc.pdf")));
}

#[test]
fn a_document_with_a_tab_brings_its_tab_back_either_way() {
    let mut tabs = tabs_of(&["a.md", "b.md"]);
    tabs.navigate("a.md");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["a.md", "b.md"], Some("a.md")), "no second a.md, b.md kept");
    tabs.open_beside("b.md");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["a.md", "b.md"], Some("b.md")));
}

#[test]
fn a_document_comes_back_where_it_was_even_after_its_tab_moved_on() {
    let mut tabs = Tabs::default();
    tabs.navigate("a.md");
    let place = TabPlace { top: (120, 0.0), cursor: Some((130, 4)) };
    tabs.remember(place);
    tabs.navigate("b.md");
    assert_eq!(tabs.navigate("a.md"), Some(place));
}

#[test]
fn closing_the_tab_shown_shows_the_next_or_the_one_before_the_last() {
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    tabs.navigate("b.md");
    assert_eq!(tabs.close(1), AfterClose::Show("c.md".into()));
    assert_eq!(tabs.close(1), AfterClose::Show("a.md".into()), "the last closed: the one before");
    assert_eq!(tabs.close(0), AfterClose::Nothing);
    assert!(tabs.is_empty());
}

#[test]
fn closing_another_tab_keeps_the_one_shown() {
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    assert_eq!(tabs.close(0), AfterClose::Same);
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["b.md", "c.md"], Some("c.md")));
}

#[test]
fn tabs_follow_renames_and_lose_deleted_documents() {
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    tabs.navigate("b.md");
    tabs.remember(TabPlace { top: (5, 0.0), cursor: None });
    tabs.navigate("c.md");
    tabs.rename("b.md", "Docs/b2.md");
    assert_eq!(tabs.place_of("Docs/b2.md"), Some(TabPlace { top: (5, 0.0), cursor: None }));
    tabs.retain(|p| p != "a.md");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["Docs/b2.md", "c.md"], Some("c.md")));
    tabs.retain(|p| p != "c.md");
    assert_eq!(tabs.current_path(), None, "the one shown was deleted");
}

#[test]
fn tabs_round_trip_through_json() {
    let mut tabs = Tabs::default();
    tabs.navigate("a.md");
    tabs.remember(TabPlace { top: (12, 0.0), cursor: Some((3, 0)) });
    tabs.open_beside("doc.pdf");
    tabs.remember(TabPlace { top: (2, 0.25), cursor: None });
    let back = Tabs::from_json(&tabs.to_json());
    assert_eq!(paths(&back), ["a.md", "doc.pdf"]);
    assert_eq!(back.current_path(), Some("doc.pdf"));
    assert_eq!(back.place_of("a.md"), Some(TabPlace { top: (12, 0.0), cursor: Some((3, 0)) }));
    assert_eq!(back.place_of("doc.pdf"), Some(TabPlace { top: (2, 0.25), cursor: None }));
    assert!(Tabs::from_json(&serde_json::json!("broken")).is_empty());
    assert_eq!(paths(&Tabs::from_text(&tabs.to_json().to_string())), ["a.md", "doc.pdf"]);
    assert!(Tabs::from_text("{not json").is_empty());
}

#[test]
fn closing_the_other_tabs_keeps_one_and_shows_it() {
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    assert_eq!(tabs.close_others(1), AfterClose::Show("b.md".into()), "c.md was shown");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["b.md"], Some("b.md")));
    let mut tabs = tabs_of(&["a.md", "b.md"]);
    assert_eq!(tabs.close_others(1), AfterClose::Same);
    assert_eq!(paths(&tabs), ["b.md"]);
}

#[test]
fn a_tab_moved_takes_its_new_place_and_the_one_shown_stays_shown() {
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    tabs.move_tab(0, 2);
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["b.md", "c.md", "a.md"], Some("c.md")), "c.md was shown, and is");
    tabs.move_tab(1, 0);
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["c.md", "b.md", "a.md"], Some("c.md")));
    tabs.move_tab(1, 1);
    tabs.move_tab(0, 9);
    tabs.move_tab(7, 0);
    assert_eq!(paths(&tabs), ["c.md", "b.md", "a.md"], "no such place: nothing moves");
}

#[test]
fn tabs_are_gone_through_by_step_and_by_number_and_found_under_a_folder() {
    let mut tabs = tabs_of(&["a.md", "Work/b.md", "Work/Sub/c.md", "Works.md"]);
    tabs.navigate("Works.md");
    // ⌃Tab and ⌃⇧Tab, round the ends.
    assert_eq!((tabs.stepped(1), tabs.stepped(-1), tabs.stepped(-5)), (Some("a.md"), Some("Work/Sub/c.md"), Some("Work/Sub/c.md")));
    // ⌘1 to ⌘8 by their place, ⌘9 the last; a number past the tabs, none.
    assert_eq!((tabs.numbered(1), tabs.numbered(4), tabs.numbered(5), tabs.numbered(9), tabs.numbered(0)), (Some("a.md"), Some("Works.md"), None, Some("Works.md"), None));
    // A folder deleted takes the tabs of what is in it, not those whose names begin as its.
    assert_eq!(tabs.under("Work"), ["Work/b.md", "Work/Sub/c.md"]);
    assert_eq!(tabs.under("a.md"), ["a.md"]);
    assert!(tabs.under("Gone").is_empty());
    // One tab, or none: nowhere to step.
    assert_eq!((tabs_of(&["a.md"]).stepped(1), Tabs::default().stepped(1), Tabs::default().numbered(9)), (None, None, None));
}

#[test]
fn the_tab_closed_last_comes_back_where_it_stood() {
    let all = |_: &str| true;
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md", "d.md"]);
    assert_eq!(tabs.reopen(all), None, "nothing closed yet");
    // b and then d closed: d comes back first, at the end again; then b, second again.
    tabs.close(1);
    tabs.close(2);
    assert_eq!(paths(&tabs), ["a.md", "c.md"]);
    assert_eq!((tabs.reopen(all).as_deref(), paths(&tabs), tabs.current_path()), (Some("d.md"), vec!["a.md", "c.md", "d.md"], Some("d.md")));
    assert_eq!((tabs.reopen(all).as_deref(), paths(&tabs), tabs.current_path()), (Some("b.md"), vec!["a.md", "b.md", "c.md", "d.md"], Some("b.md")));
    assert_eq!(tabs.reopen(all), None);
    // Where it was when left is kept, as for any document.
    tabs.navigate("c.md");
    tabs.remember(TabPlace { top: (40, 0.0), cursor: Some((7, 0)) });
    tabs.close(2);
    assert_eq!(tabs.reopen(all).as_deref(), Some("c.md"));
    assert_eq!(tabs.place_of("c.md"), Some(TabPlace { top: (40, 0.0), cursor: Some((7, 0)) }));
    // A document gone since, or in a tab again, is passed over for the one closed before it.
    tabs.close(0);
    tabs.close(0);
    tabs.close(0);
    assert_eq!(paths(&tabs), ["d.md"]);
    tabs.open_beside("b.md");
    assert_eq!(tabs.reopen(|path| path != "c.md").as_deref(), Some("a.md"), "c is gone from the disk, b has a tab already");
    assert_eq!(paths(&tabs), ["a.md", "d.md", "b.md"]);
    // A document renamed while its tab was closed comes back under its new name.
    tabs.close(0);
    tabs.rename("a.md", "A.md");
    assert_eq!(tabs.reopen(all).as_deref(), Some("A.md"));
    // Every other tab closed at once: they come back one by one, each to its place.
    let mut tabs = tabs_of(&["a.md", "b.md", "c.md"]);
    tabs.close_others(1);
    assert_eq!(paths(&tabs), ["b.md"]);
    tabs.reopen(all);
    tabs.reopen(all);
    assert_eq!(paths(&tabs), ["a.md", "b.md", "c.md"]);
    // The last tab closed leaves none; it comes back as the only one.
    let mut tabs = tabs_of(&["a.md"]);
    assert_eq!(tabs.close(0), AfterClose::Nothing);
    assert_eq!((tabs.reopen(all).as_deref(), tabs.current()), (Some("a.md"), Some(0)));
}

