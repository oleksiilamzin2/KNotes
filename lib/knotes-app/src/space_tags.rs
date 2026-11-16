use std::collections::HashSet;

use crate::{SpaceFolders, TagNode, split_tags};

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
}
