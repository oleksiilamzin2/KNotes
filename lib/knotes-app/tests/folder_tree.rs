use knotes_app::{FolderNode, folder_tree};

fn names(node: &FolderNode) -> Vec<(&str, usize)> {
    node.children.iter().map(|c| (c.name.as_str(), c.notes)).collect()
}

#[test]
fn counts_notes_recursively_and_shows_attachment_only_folders() {
    let root = folder_tree(["a.md", "Projects/Radar/x.md", "Projects/y.md", "Projects/Radar/z.md", "att/pic.png"]);
    assert_eq!((root.path.as_str(), root.notes), ("", 4));
    assert_eq!(names(&root), [("att", 0), ("Projects", 3)]);
    let projects = &root.children[1];
    assert_eq!(projects.path, "Projects");
    assert_eq!(names(projects), [("Radar", 2)]);
    assert_eq!(projects.children[0].path, "Projects/Radar");
}

#[test]
fn children_sort_naturally_and_ignore_case() {
    let root = folder_tree(["Folder 10/a.md", "folder 2/a.md", "Folder 1/a.md", "Архів/a.md", "Біологія/a.md", "beta/a.md"]);
    let order: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(order, ["beta", "Folder 1", "folder 2", "Folder 10", "Архів", "Біологія"]);
}

#[test]
fn empty_vault() {
    let root = folder_tree(std::iter::empty::<&str>());
    assert_eq!((root.notes, root.children.len()), (0, 0));
}

#[test]
fn the_hidden_attachments_folder_is_no_folder_in_the_tree() {
    let root = folder_tree(["a.md", ".knotes/attachments/Pasted image 20261113114500.png"]);
    assert_eq!(names(&root), []);
}

#[test]
fn in_a_space_the_tree_splits_into_its_folders_and_the_rest_each_folder_in_one_part() {
    use knotes_app::SpaceFolders;
    use knotes_spaces::Spaces;
    let paths = ["Work/a.md", "Work/Radar/b.md", "Work/Radar/c.md", "Work/Archive/d.md", "Work/Archive/Keep/h.md", "Home/e.md", "top.md", "Areas/f.md", "Areas/Job/g.md"];
    let tree = folder_tree(paths);
    let spaces = Spaces::from_texts(&[
        (String::new(), r#"{"spaces": [{"id": "work"}]}"#.into()),
        ("Work".into(), r#"{"space": "work"}"#.into()),
        ("Work/Archive".into(), r#"{"space": null}"#.into()),
        ("Work/Archive/Keep".into(), r#"{"space": "work"}"#.into()),
        ("Areas/Job".into(), r#"{"space": "work"}"#.into()),
    ]);
    let (space, rest) = tree.split(&SpaceFolders::new(&tree, &spaces, "work"));
    assert_eq!(names(&space), [("Areas/Job", 1), ("Work", 4)], "a space's folder inside another part's at the top, named by its path");
    let work = space.children.iter().find(|c| c.path == "Work").unwrap();
    assert_eq!(names(work), [("Archive/Keep", 1), ("Radar", 2)], "back in the space below a taken-out folder: hangs from Work");
    assert_eq!(space.notes, 5);
    assert_eq!(names(&rest), [("Areas", 1), ("Home", 1), ("Work/Archive", 1)], "Areas keeps its own note, Job gone up");
    assert_eq!(rest.notes, 4, "with the root's note");
    let own = |n: &FolderNode| n.notes - n.children.iter().map(|c| c.notes).sum::<usize>();
    assert_eq!(own(work), 1, "a folder's own notes are its count less its children's, as the sidebar takes them");
}

#[test]
fn empty_folders_are_in_the_tree_with_no_notes() {
    use knotes_app::folder_tree_with;
    let tree = folder_tree_with(["Work/a.md"], ["Empty", "Work/New/Deeper", "Work"]);
    assert_eq!(names(&tree), [("Empty", 0), ("Work", 1)]);
    let work = &tree.children[1];
    assert_eq!(names(work), [("New", 0)]);
    assert_eq!(names(&work.children[0]), [("Deeper", 0)]);
}
