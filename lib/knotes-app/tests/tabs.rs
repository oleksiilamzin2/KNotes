use knotes_app::{AfterClose, TabPlace, Tabs};

fn paths(tabs: &Tabs) -> Vec<&str> {
    tabs.paths().collect()
}

#[test]
fn every_document_opened_is_a_tab_and_one_opened_again_brings_its_tab_back() {
    let mut tabs = Tabs::default();
    assert_eq!(tabs.open("a.md"), None);
    tabs.open("b.md");
    tabs.open("doc.pdf");
    assert_eq!(paths(&tabs), ["a.md", "b.md", "doc.pdf"]);
    tabs.open("a.md");
    assert_eq!(paths(&tabs), ["a.md", "b.md", "doc.pdf"], "no second tab");
    assert_eq!(tabs.current_path(), Some("a.md"));
}

#[test]
fn a_tab_remembers_where_its_document_was() {
    let mut tabs = Tabs::default();
    tabs.open("a.md");
    let place = TabPlace { top: (120, 0.0), cursor: Some((130, 4)) };
    tabs.remember(place);
    tabs.open("b.md");
    assert_eq!(tabs.open("a.md"), Some(place));
}

#[test]
fn closing_the_tab_shown_shows_the_next_or_the_one_before_the_last() {
    let mut tabs = Tabs::default();
    for p in ["a.md", "b.md", "c.md"] {
        tabs.open(p);
    }
    tabs.open("b.md");
    assert_eq!(tabs.close(1), AfterClose::Show("c.md".into()));
    assert_eq!(tabs.close(1), AfterClose::Show("a.md".into()), "the last closed: the one before");
    assert_eq!(tabs.close(0), AfterClose::Nothing);
    assert!(tabs.is_empty());
}

#[test]
fn closing_another_tab_keeps_the_one_shown() {
    let mut tabs = Tabs::default();
    for p in ["a.md", "b.md", "c.md"] {
        tabs.open(p);
    }
    assert_eq!(tabs.close(0), AfterClose::Same);
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["b.md", "c.md"], Some("c.md")));
}

#[test]
fn tabs_follow_renames_and_lose_deleted_documents() {
    let mut tabs = Tabs::default();
    for p in ["a.md", "b.md", "c.md"] {
        tabs.open(p);
    }
    tabs.rename("b.md", "Docs/b2.md");
    tabs.retain(|p| p != "a.md");
    assert_eq!((paths(&tabs), tabs.current_path()), (vec!["Docs/b2.md", "c.md"], Some("c.md")));
    tabs.retain(|p| p != "c.md");
    assert_eq!(tabs.current_path(), None, "the one shown was deleted");
}

#[test]
fn tabs_round_trip_through_json() {
    let mut tabs = Tabs::default();
    tabs.open("a.md");
    tabs.remember(TabPlace { top: (12, 0.0), cursor: Some((3, 0)) });
    tabs.open("doc.pdf");
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
