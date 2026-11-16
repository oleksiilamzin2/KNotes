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

#[test]
fn in_a_space_the_tags_split_into_its_own_and_those_only_outside_it() {
    use knotes_app::split_tags;
    // A note's tags together, as the index gives them.
    let tagged: Vec<(String, String)> = [("radar", "Work/a.md"), ("area/job", "Work/a.md"), ("radar", "Home/b.md"), ("area/home", "Home/b.md"), ("cooking", "Home/c.md"), ("cooking", "top.md")]
        .iter()
        .map(|(t, p)| (t.to_string(), p.to_string()))
        .collect();
    let (space, rest) = split_tags(&tagged, &|path: &str| path.starts_with("Work/"));
    let shown = |nodes: &[TagNode]| nodes.iter().map(|n| (n.name.clone(), n.notes, n.children.iter().map(|c| (c.name.clone(), c.notes)).collect::<Vec<_>>())).collect::<Vec<_>>();
    assert_eq!(shown(&space), [("area".into(), 1, vec![("job".into(), 1)]), ("radar".into(), 1, vec![])], "counted within the space");
    assert_eq!(shown(&rest), [("area/home".into(), 1, vec![]), ("cooking".into(), 2, vec![])], "only outside it; a child whose parent is in the space at the top, by its whole tag");
}

#[test]
fn tags_count_from_each_note_s_tags_once_a_note_for_a_tag_and_every_parent() {
    use knotes_app::count_tags;
    let tagged = [("project/radar", "a.md"), ("project/ui", "a.md"), ("todo", "a.md"), ("project", "b.md"), ("project/radar/fmcw", "c.md")];
    let counts = count_tags(tagged.iter().copied());
    let counts: Vec<(&str, usize)> = counts.iter().map(|(t, n)| (t.as_str(), *n)).collect();
    assert_eq!(counts, [("project", 3), ("project/radar", 2), ("project/radar/fmcw", 1), ("project/ui", 1), ("todo", 1)], "sorted by tag; a.md once under «project»");
}
