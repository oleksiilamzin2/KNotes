use knotes_app::{TagNode, tag_tree};

#[test]
fn nests_tags_by_slash_with_counts() {
    let counts: Vec<(String, usize)> =
        [("project", 3), ("project/radar", 2), ("project/radar/fmcw", 1), ("project/ui", 2), ("todo", 1)]
            .map(|(t, n)| (t.to_owned(), n))
            .to_vec();
    let tree = tag_tree(&counts);
    let top: Vec<(&str, usize)> = tree.iter().map(|n| (n.name.as_str(), n.notes)).collect();
    assert_eq!(top, [("project", 3), ("todo", 1)]);
    let project: &TagNode = &tree[0];
    let kids: Vec<(&str, &str, usize)> = project.children.iter().map(|n| (n.tag.as_str(), n.name.as_str(), n.notes)).collect();
    assert_eq!(kids, [("project/radar", "radar", 2), ("project/ui", "ui", 2)]);
    assert_eq!(project.children[0].children[0].tag, "project/radar/fmcw");
}
