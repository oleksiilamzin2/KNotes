use knotes_app::{Sidebar, SidebarItem, folder_tree, tag_tree};

fn sidebar() -> Sidebar {
    let folders = folder_tree(["a.md", "Projects/Radar/x.md", "Projects/y.md", "Inbox/z.md"]);
    let tags = tag_tree(&[("project".to_owned(), 2), ("project/radar".to_owned(), 1), ("todo".to_owned(), 1)]);
    Sidebar::new(&folders, &tags)
}

#[test]
fn top_level_is_all_notes_then_folders_and_tags_sections() {
    let s = sidebar();
    assert_eq!(s.children(None), [SidebarItem::AllNotes, SidebarItem::FoldersSection, SidebarItem::TagsSection]);
    assert_eq!(s.count(&SidebarItem::AllNotes), Some(4));
    assert_eq!(s.count(&SidebarItem::FoldersSection), None);
}

#[test]
fn folders_and_tags_nest_with_counts() {
    let s = sidebar();
    let folders = s.children(Some(&SidebarItem::FoldersSection));
    assert_eq!(folders, [SidebarItem::Folder("Inbox".into()), SidebarItem::Folder("Projects".into())]);
    assert_eq!(s.children(Some(&SidebarItem::Folder("Projects".into()))), [SidebarItem::Folder("Projects/Radar".into())]);
    assert_eq!(s.count(&SidebarItem::Folder("Projects".into())), Some(2));
    assert_eq!(s.name(&SidebarItem::Folder("Projects/Radar".into())), "Radar");
    let tags = s.children(Some(&SidebarItem::TagsSection));
    assert_eq!(tags, [SidebarItem::Tag("project".into()), SidebarItem::Tag("todo".into())]);
    assert_eq!(s.children(Some(&SidebarItem::Tag("project".into()))), [SidebarItem::Tag("project/radar".into())]);
    assert_eq!(s.name(&SidebarItem::Tag("project/radar".into())), "radar");
}

#[test]
fn items_round_trip_through_their_keys() {
    for item in [SidebarItem::AllNotes, SidebarItem::FoldersSection, SidebarItem::Folder("Проєкти/Радар".into()), SidebarItem::Tag("a/b".into())] {
        assert_eq!(SidebarItem::from_key(&item.key()), Some(item));
    }
    assert_eq!(SidebarItem::from_key("bogus"), None);
}
