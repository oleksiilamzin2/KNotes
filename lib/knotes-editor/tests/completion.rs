use knotes_editor::{LinkCompletion, link_completion};

#[test]
fn an_open_wikilink_completes_a_title_from_after_the_brackets() {
    assert_eq!(link_completion("see [[Рад"), Some(LinkCompletion::Title { from: 6, query: "Рад".into() }));
    assert_eq!(link_completion("[["), Some(LinkCompletion::Title { from: 2, query: String::new() }));
    assert_eq!(link_completion("embed ![[pic"), Some(LinkCompletion::Title { from: 9, query: "pic".into() }));
}

#[test]
fn after_a_hash_it_completes_a_heading_of_the_target() {
    assert_eq!(
        link_completion("[[Note#Дру"),
        Some(LinkCompletion::Heading { from: 7, target: "Note".into(), query: "Дру".into() })
    );
    assert_eq!(link_completion("[[#"), Some(LinkCompletion::Heading { from: 3, target: String::new(), query: String::new() }));
}

#[test]
fn closed_links_aliases_and_plain_text_complete_nothing() {
    assert_eq!(link_completion("see [[Note]] and"), None);
    assert_eq!(link_completion("[[Note|ali"), None);
    assert_eq!(link_completion("plain [text"), None);
    assert_eq!(link_completion("[[a]] then [[b"), Some(LinkCompletion::Title { from: 13, query: "b".into() }));
}

#[test]
fn after_hash_caret_the_blocks_of_the_target_are_completed() {
    assert_eq!(link_completion("see [[Note#^ab"), Some(LinkCompletion::Block { from: 11, target: "Note".into(), query: "ab".into() }));
    assert_eq!(link_completion("[[#^"), Some(LinkCompletion::Block { from: 3, target: String::new(), query: String::new() }));
}
