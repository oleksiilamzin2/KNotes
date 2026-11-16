use std::collections::{HashMap, HashSet};

use crate::natural_order::natural_cmp;

/// A tag in the sidebar; nested tags (`area/sub`) hang below their parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagNode {
    /// Full tag without `#`, e.g. `project/radar`.
    pub tag: String,
    /// Last segment, e.g. `radar`.
    pub name: String,
    /// Distinct notes with this tag or one below it.
    pub notes: usize,
    pub children: Vec<TagNode>,
}

/// `counts` as given by `TagIndex::tag_counts`: every tag and every parent, sorted by tag.
pub fn tag_tree(counts: &[(String, usize)]) -> Vec<TagNode> {
    let mut roots: Vec<TagNode> = Vec::new();
    for (tag, notes) in counts {
        let node = TagNode { tag: tag.clone(), name: tag.rsplit('/').next().unwrap_or(tag).to_owned(), notes: *notes, children: Vec::new() };
        // Sorted input puts every parent before its children, so the parent chain exists.
        let mut level = &mut roots;
        let mut end = 0;
        while let Some(slash) = tag[end..].find('/') {
            let parent = &tag[..end + slash];
            end += slash + 1;
            match level.iter().position(|n| n.tag == parent) {
                Some(i) => level = &mut level[i].children,
                None => break,
            }
        }
        level.push(node);
    }
    sort(&mut roots);
    roots
}

fn sort(nodes: &mut [TagNode]) {
    nodes.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    for n in nodes {
        sort(&mut n.children);
    }
}

/// Every tag and every parent with how many notes have it or one below it, sorted by tag, from
/// (tag, note) pairs that keep a note's tags together (as `TagIndex::tagged_notes` gives them).
pub fn count_tags<'a>(tagged: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<(String, usize)> {
    // A tag's count and the last note counted for it: a note's tags come together, so a note
    // with two tags below one parent counts there once, with no set of notes a tag.
    let mut counts: HashMap<&str, (usize, &str)> = HashMap::new();
    let mut count = |tag: &'a str, path: &'a str| {
        let (notes, last) = counts.entry(tag).or_insert((0, ""));
        if *last != path {
            *notes += 1;
            *last = path;
        }
    };
    for (tag, path) in tagged {
        for (i, _) in tag.match_indices('/') {
            count(&tag[..i], path);
        }
        count(tag, path);
    }
    let mut counts: Vec<(String, usize)> = counts.into_iter().map(|(tag, (notes, _))| (tag.to_owned(), notes)).collect();
    counts.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    counts
}

/// The tags in a space and the rest (ADR 0030), from every (tag, note) pair, a note's together:
/// the tags with notes in the space (`within`), counted there; then those with none there,
/// counted outside, one whose parent is in the space at the top under its whole tag.
pub fn split_tags(tagged: &[(String, String)], within: &dyn Fn(&str) -> bool) -> (Vec<TagNode>, Vec<TagNode>) {
    // Asked once a note: its tags come together.
    let mut last: Option<(&str, bool)> = None;
    let mut inside = Vec::new();
    for (tag, path) in tagged {
        let held = match last {
            Some((at, held)) if at == path => held,
            _ => {
                let held = within(path);
                last = Some((path, held));
                held
            }
        };
        if held {
            inside.push((tag.as_str(), path.as_str()));
        }
    }
    let all = count_tags(tagged.iter().map(|(tag, path)| (tag.as_str(), path.as_str())));
    space_tag_trees(&count_tags(inside), &all)
}

/// A space's tags from its counts (`count_tags` of its notes'), and the rest from the vault's
/// (`all`): those with no note in the space, so counted as in the vault, one whose parent is in
/// the space at the top under its whole tag.
pub fn space_tag_trees(inside: &[(String, usize)], all: &[(String, usize)]) -> (Vec<TagNode>, Vec<TagNode>) {
    let in_space: HashSet<&str> = inside.iter().map(|(tag, _)| tag.as_str()).collect();
    let outside: Vec<(String, usize)> = all.iter().filter(|(tag, _)| !in_space.contains(tag.as_str())).cloned().collect();
    let mut rest = tag_tree(&outside);
    for node in &mut rest {
        node.name = node.tag.clone();
    }
    rest.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    (tag_tree(inside), rest)
}
