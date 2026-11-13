use knotes_app::{Sidebar, SidebarItem, SidebarMode, folder_tree, tag_tree};

fn sidebar() -> Sidebar {
    sidebar_in(SidebarMode::Folders)
}

fn sidebar_in(mode: SidebarMode) -> Sidebar {
    let folders = folder_tree(["a.md", "Projects/Radar/x.md", "Projects/y.md", "Inbox/z.md"]);
    let tags = tag_tree(&[("project".to_owned(), 2), ("project/radar".to_owned(), 1), ("todo".to_owned(), 1)]);
    Sidebar::new(&folders, &tags, mode, None, 0, &|_| true)
}

#[test]
fn top_level_is_all_notes_then_the_top_folders_or_tags_without_a_heading() {
    let s = sidebar();
    let fixed = [SidebarItem::AllNotes, SidebarItem::Untagged, SidebarItem::Gap];
    assert_eq!(s.children(None)[..3], fixed, "no inbox row when there is no inbox folder");
    assert_eq!(s.children(None)[3..], [SidebarItem::Folder("Inbox".into()), SidebarItem::Folder("Projects".into())]);
    let tags = sidebar_in(SidebarMode::Tags);
    assert_eq!(tags.children(None)[3..], [SidebarItem::Tag("project".into()), SidebarItem::Tag("todo".into())]);
    assert_eq!(s.count(&SidebarItem::AllNotes), Some(4));
}

#[test]
fn folders_and_tags_nest_with_counts() {
    let s = sidebar();
    assert_eq!(s.children(Some(&SidebarItem::Folder("Projects".into()))), [SidebarItem::Folder("Projects/Radar".into())]);
    assert_eq!(s.count(&SidebarItem::Folder("Projects".into())), Some(2));
    assert_eq!(s.name(&SidebarItem::Folder("Projects/Radar".into())), "Radar");
    let s = sidebar_in(SidebarMode::Tags);
    assert_eq!(s.children(Some(&SidebarItem::Tag("project".into()))), [SidebarItem::Tag("project/radar".into())]);
    assert_eq!(s.name(&SidebarItem::Tag("project/radar".into())), "radar");
}

#[test]
fn items_round_trip_through_their_keys() {
    for item in [SidebarItem::AllNotes, SidebarItem::Untagged, SidebarItem::Gap, SidebarItem::Folder("Проєкти/Радар".into()), SidebarItem::Tag("a/b".into())] {
        assert_eq!(SidebarItem::from_key(&item.key()), Some(item));
    }
    assert_eq!(SidebarItem::from_key("bogus"), None);
}

fn with_inbox(mode: SidebarMode, inbox: &str) -> Sidebar {
    let folders = folder_tree(["a.md", "Projects/Radar/x.md", "Projects/y.md", "Inbox/z.md", "Inbox/Old/w.md"]);
    let tags = tag_tree(&[("todo".to_owned(), 1)]);
    Sidebar::new(&folders, &tags, mode, Some(inbox), 3, &|_| true)
}

#[test]
fn the_inbox_is_pinned_under_all_notes_then_a_gap_then_the_tree_without_it() {
    let s = with_inbox(SidebarMode::Folders, "Inbox");
    let inbox = SidebarItem::Folder("Inbox".into());
    assert_eq!(
        s.children(None),
        [SidebarItem::AllNotes, inbox.clone(), SidebarItem::Untagged, SidebarItem::Gap, SidebarItem::Folder("Projects".into())]
    );
    assert_eq!(s.count(&SidebarItem::Untagged), Some(3));
    assert_eq!(s.inbox(), Some(&inbox));
    assert_eq!((s.name(&inbox), s.count(&inbox)), ("Inbox", Some(2)));
    assert_eq!(s.children(Some(&inbox)), [SidebarItem::Folder("Inbox/Old".into())], "its subfolders stay reachable");
    let tags = with_inbox(SidebarMode::Tags, "Inbox");
    assert_eq!(tags.children(None), [SidebarItem::AllNotes, inbox, SidebarItem::Untagged, SidebarItem::Gap, SidebarItem::Tag("todo".into())]);
}

#[test]
fn the_inbox_shows_before_its_folder_exists_and_may_be_nested() {
    let s = with_inbox(SidebarMode::Folders, "Вхідні");
    let inbox = SidebarItem::Folder("Вхідні".into());
    assert_eq!(s.children(None)[1], inbox);
    assert_eq!((s.name(&inbox), s.count(&inbox)), ("Вхідні", Some(0)));
    let nested = with_inbox(SidebarMode::Folders, "Projects/Radar");
    assert_eq!(nested.name(&SidebarItem::Folder("Projects/Radar".into())), "Radar");
    assert!(nested.children(Some(&SidebarItem::Folder("Projects".into()))).is_empty(), "not listed twice");
}

#[test]
fn a_folder_counts_the_notes_its_list_shows() {
    let folders = folder_tree(["Projects/Radar/x.md", "Projects/y.md", "Inbox/z.md", "Inbox/Old/w.md"]);
    let own = Sidebar::new(&folders, &[], SidebarMode::Folders, Some("Inbox"), 0, &|_| false);
    assert_eq!(own.count(&SidebarItem::Folder("Projects".into())), Some(1), "its own notes, not its subfolders'");
    assert_eq!(own.count(&SidebarItem::Folder("Inbox".into())), Some(1));
    assert_eq!(own.count(&SidebarItem::AllNotes), Some(4));
    let mixed = Sidebar::new(&folders, &[], SidebarMode::Folders, None, 0, &|path| path == "Projects");
    assert_eq!(mixed.count(&SidebarItem::Folder("Projects".into())), Some(2), "a folder that shows its subfolders counts them");
}

#[test]
fn the_tabs_mode_lists_a_row_per_tab_with_its_name() {
    use knotes_app::{Sidebar, SidebarItem, SidebarMode};
    let sidebar = Sidebar::tabs(&[("a.md".into(), "a".into()), ("Docs/doc.pdf".into(), "doc".into())]);
    let rows = sidebar.children(None);
    assert_eq!(rows, [SidebarItem::Tab("a.md".into()), SidebarItem::Tab("Docs/doc.pdf".into())]);
    assert_eq!((sidebar.name(&rows[1]), sidebar.count(&rows[1])), ("doc", None));
    assert_eq!(SidebarItem::from_key(&rows[1].key()), Some(rows[1].clone()));
    assert_eq!(SidebarMode::from_key(SidebarMode::Tabs.key()), Some(SidebarMode::Tabs));
}
