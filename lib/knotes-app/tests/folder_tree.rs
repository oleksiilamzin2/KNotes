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
