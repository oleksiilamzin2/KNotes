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
