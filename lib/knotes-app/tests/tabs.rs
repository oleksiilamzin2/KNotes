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
