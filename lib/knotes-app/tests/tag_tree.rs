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

#[test]
fn tags_are_offered_the_space_s_first_then_by_how_they_match_then_the_most_used() {
    use knotes_app::suggest_tags;
    let counts = |pairs: &[(&str, usize)]| tag_tree(&pairs.iter().map(|(t, n)| (t.to_string(), *n)).collect::<Vec<_>>());
    let all = counts(&[("area", 2), ("area/home", 1), ("area/job", 1), ("cooking", 4), ("project", 2), ("project/radar", 2), ("radar", 5), ("radio", 1)]);
    let space = counts(&[("area", 1), ("area/job", 1), ("radar", 3)]);
    assert_eq!(suggest_tags("ra", &[], &all, 10), [("radar", 5), ("radio", 1), ("project/radar", 2)], "the start first, then a part's start");
    assert_eq!(suggest_tags("Ra", &space, &all, 10), [("radar", 3), ("radio", 1), ("project/radar", 2)], "the space's first, counted there; case ignored");
    assert_eq!(suggest_tags("", &space, &all, 3), [("radar", 3), ("area", 1), ("area/job", 1)], "nothing typed: the most used, at most the limit");
    assert_eq!(suggest_tags("adi", &[], &all, 10), [("radio", 1)], "held inside");
}

#[test]
fn a_level_typed_out_offers_the_tags_right_under_it_a_step_at_a_time() {
    use knotes_app::{has_nested_tags, suggest_tags};
    let counts = |pairs: &[(&str, usize)]| tag_tree(&pairs.iter().map(|(t, n)| (t.to_string(), *n)).collect::<Vec<_>>());
    let all = counts(&[("project", 6), ("project/radar", 3), ("project/radar/fmcw", 2), ("project/radar/pulse", 1), ("project/ui", 4), ("project/quarry", 1), ("radar", 5)]);
    assert_eq!(suggest_tags("project/", &[], &all, 10), [("project/ui", 4), ("project/radar", 3), ("project/quarry", 1)], "its own level alone, the most used first");
    assert_eq!(suggest_tags("project/ra", &[], &all, 10), [("project/radar", 3)], "not the tags under the one named");
    assert_eq!(suggest_tags("project/r", &[], &all, 10), [("project/radar", 3), ("project/quarry", 1)], "a name's start first, then held inside");
    assert_eq!(suggest_tags("Project/Radar/", &[], &all, 10), [("project/radar/fmcw", 2), ("project/radar/pulse", 1)], "a step further; case ignored");
    // A level with nothing such under it: matched whole, as a tag without levels is.
    assert_eq!(suggest_tags("radar/", &[], &all, 10), [("project/radar/fmcw", 2), ("project/radar/pulse", 1)], "held inside");
    assert!(suggest_tags("project/zz", &[], &all, 10).is_empty());
    // In a space its own tags of the level first.
    let space = counts(&[("project", 1), ("project/quarry", 1)]);
    assert_eq!(suggest_tags("project/", &space, &all, 10), [("project/quarry", 1), ("project/ui", 4), ("project/radar", 3)]);
    assert!(has_nested_tags(&all, "project") && has_nested_tags(&all, "project/radar") && !has_nested_tags(&all, "project/ui") && !has_nested_tags(&all, "none"));
}
