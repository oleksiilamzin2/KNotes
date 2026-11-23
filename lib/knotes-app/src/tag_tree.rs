use std::collections::{HashMap, HashSet};

use knotes_core::natural_cmp;

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

/// The decks of the vault's cards as a tree (ADR 0037), as tags nest: each deck, and each
/// parent a path names, with the cards to review in it and under it: those due, and of those
/// never reviewed as many as the deck still asks today. `decks`: each deck's own (`DeckCount`),
/// in any order; `introduced`: each deck's own new cards first reviewed today; `new_a_day`: how
/// many new cards a day a deck asks. Of the due whose days have begun, what the deck's day
/// still takes: `reviewed`, each deck's own reviewed today; `reviews_a_day`, how many a day a
/// deck asks (none: all). Those still at a step are all counted.
pub fn deck_tree(decks: &[knotes_core::DeckCount], introduced: &[(String, usize)], new_a_day: &dyn Fn(&str) -> usize, reviewed: &[(String, usize)], reviews_a_day: &dyn Fn(&str) -> Option<usize>) -> Vec<TagNode> {
    // Of a deck with those under it: the due, the never reviewed, today's new, the due still at
    // a step, today's reviews.
    let mut counts: std::collections::BTreeMap<String, [usize; 5]> = std::collections::BTreeMap::new();
    let mut add = |deck: &str, which: usize, n: usize| {
        for at in deck.match_indices('/').map(|(i, _)| i).chain([deck.len()]) {
            counts.entry(deck[..at].to_owned()).or_default()[which] += n;
        }
    };
    for deck in decks {
        add(&deck.deck, 0, deck.due);
        add(&deck.deck, 1, deck.fresh);
        add(&deck.deck, 3, deck.learning);
    }
    for (deck, n) in reviewed.iter().filter(|(deck, _)| decks.iter().any(|d| d.deck == *deck)) {
        add(deck, 4, *n);
    }
    // A deck whose cards are gone has none to count.
    for (deck, n) in introduced.iter().filter(|(deck, _)| decks.iter().any(|d| d.deck == *deck)) {
        add(deck, 2, *n);
    }
    // Every parent before its children, as `tag_tree` takes them.
    let counts: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(deck, [due, fresh, today, learning, reviewed])| {
            let begun = due - learning;
            let asked = reviews_a_day(&deck).map_or(begun, |a_day| begun.min(a_day.saturating_sub(reviewed)));
            let waiting = learning + asked + fresh.min(new_a_day(&deck).saturating_sub(today));
            (deck, waiting)
        })
        .collect();
    tag_tree(&counts)
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

/// The tags to offer for `query` while one is typed (ADR 0033), at most `limit`, with their
/// notes: in a space its own first (`space`, counted there), then the rest of the vault's
/// (`all`); in each, those starting with it, then those with a part (after `/`) starting with
/// it, then those holding it; the most used first. Case is ignored (the index keeps tags
/// lowercased).
///
/// A level typed out (`project/`, `project/ra`) is a step into it: the tags right under it
/// alone, those whose own name starts with what follows first, then those holding it. Where it
/// has none such (a level that is not there, a new tag), the tags are matched whole as above.
pub fn suggest_tags<'a>(query: &str, space: &'a [TagNode], all: &'a [TagNode], limit: usize) -> Vec<(&'a str, usize)> {
    let query = query.to_lowercase();
    if let Some((under, name)) = query.rsplit_once('/') {
        let stepped = matching_tags(space, all, limit, |node| {
            let inside = node.tag.rsplit_once('/').is_some_and(|(parent, _)| parent == under);
            let class = if node.name.starts_with(name) { Some(0) } else { node.name.contains(name).then_some(1) };
            class.filter(|_| inside)
        });
        if !stepped.is_empty() {
            return stepped;
        }
    }
    matching_tags(space, all, limit, |node| {
        let tag = node.tag.as_str();
        if tag.starts_with(&query) {
            Some(0)
        } else if tag.split('/').skip(1).any(|part| part.starts_with(&query)) {
            Some(1)
        } else {
            tag.contains(&query).then_some(2)
        }
    })
}

/// Whether tags are nested under `tag` in `nodes`: a step into it has something to offer.
pub fn has_nested_tags(nodes: &[TagNode], tag: &str) -> bool {
    let mut nested = false;
    walk(nodes, &mut |node| nested |= node.tag == tag && !node.children.is_empty());
    nested
}

/// The tags `how` gives a class to (the lower the better), at most `limit`, with their notes:
/// the space's first, then the rest of the vault's; in each by class, then the most used.
fn matching_tags<'a>(space: &'a [TagNode], all: &'a [TagNode], limit: usize, how: impl Fn(&TagNode) -> Option<u8>) -> Vec<(&'a str, usize)> {
    let mut own: HashSet<&str> = HashSet::new();
    let mut found: Vec<(u8, u8, &TagNode)> = Vec::new();
    for (group, nodes) in [(0, space), (1, all)] {
        walk(nodes, &mut |node| {
            if group == 0 {
                own.insert(&node.tag);
            } else if own.contains(node.tag.as_str()) {
                return;
            }
            if let Some(class) = how(node) {
                found.push((group, class, node));
            }
        });
    }
    found.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then(b.2.notes.cmp(&a.2.notes)).then_with(|| a.2.tag.cmp(&b.2.tag)));
    found.into_iter().take(limit).map(|(_, _, node)| (node.tag.as_str(), node.notes)).collect()
}

/// Every node below `nodes`, parents before their children.
fn walk<'a>(nodes: &'a [TagNode], visit: &mut impl FnMut(&'a TagNode)) {
    for node in nodes {
        visit(node);
        walk(&node.children, visit);
    }
}
