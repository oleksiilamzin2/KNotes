use std::collections::{HashMap, HashSet};

use crate::{SpaceFolders, TagNode, count_tags, space_tag_trees, split_tags};

/// What the sidebar shows of a space's tags (ADR 0030): its tags, those only outside it and its
/// untagged notes, counted with each snapshot (not on the main thread), with the folders they
/// were counted for, so they are counted again only when those or the tags change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceTags {
    pub id: String,
    pub folders: SpaceFolders,
    pub tags: Vec<TagNode>,
    pub rest: Vec<TagNode>,
    pub untagged: usize,
}

impl SpaceTags {
    /// The space `id`'s, from every tag with its note (a note's together) and the untagged notes.
    pub fn count(id: &str, folders: SpaceFolders, tagged: &[(String, String)], untagged: &HashSet<String>) -> Self {
        let (tags, rest) = split_tags(tagged, &|path| folders.holds(path));
        let untagged = untagged.iter().filter(|path| folders.holds(path)).count();
        Self { id: id.to_owned(), folders, tags, rest, untagged }
    }

    /// Every space's (`spaces`: each id with its folders), in one pass over the tags whatever
    /// their number: a note is in one space, or in each (a folder in every space). `all`: the
    /// vault's tags as `count_tags` counts them.
    pub fn count_all(spaces: Vec<(String, SpaceFolders)>, tagged: &[(String, String)], untagged: &HashSet<String>, all: &[(String, usize)]) -> Vec<Self> {
        // The space of each of their folders: one, or every space (in each one's folders).
        let mut member: HashMap<&str, Member> = HashMap::new();
        for (i, (_, folders)) in spaces.iter().enumerate() {
            for folder in folders.folders() {
                member.entry(folder).and_modify(|m| *m = Member::Every).or_insert(Member::One(i));
            }
        }
        let of = |path: &str| path.rsplit_once('/').and_then(|(folder, _)| member.get(folder).copied());
        let mut inside: Vec<Vec<(&str, &str)>> = vec![Vec::new(); spaces.len()];
        // Asked once a note: its tags come together.
        let mut last: Option<(&str, Option<Member>)> = None;
        for (tag, path) in tagged {
            let held = match last {
                Some((at, held)) if at == path => held,
                _ => {
                    let held = of(path);
                    last = Some((path, held));
                    held
                }
            };
            match held {
                Some(Member::One(i)) => inside[i].push((tag.as_str(), path.as_str())),
                Some(Member::Every) => inside.iter_mut().for_each(|pairs| pairs.push((tag.as_str(), path.as_str()))),
                None => {}
            }
        }
        let mut untagged_in = vec![0; spaces.len()];
        for path in untagged {
            match of(path) {
                Some(Member::One(i)) => untagged_in[i] += 1,
                Some(Member::Every) => untagged_in.iter_mut().for_each(|n| *n += 1),
                None => {}
            }
        }
        spaces
            .into_iter()
            .zip(inside)
            .zip(untagged_in)
            .map(|(((id, folders), pairs), untagged)| {
                let (tags, rest) = space_tag_trees(&count_tags(pairs), all);
                Self { id, folders, tags, rest, untagged }
            })
            .collect()
    }
}

/// The space a folder is in, for `count_all`.
#[derive(Debug, Clone, Copy)]
enum Member {
    One(usize),
    Every,
}
