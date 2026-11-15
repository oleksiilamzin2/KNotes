use std::collections::HashSet;

use knotes_spaces::Spaces;

use crate::FolderNode;

/// The folders of one space (ADR 0030), as the folder tree writes their paths: what a list in
/// the space keeps. Built once a snapshot, so a list asks a set, not the rules, of each note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpaceFolders {
    folders: HashSet<String>,
}

impl SpaceFolders {
    /// The folders of `tree` in the space `id`.
    pub fn new(tree: &FolderNode, spaces: &Spaces, id: &str) -> Self {
        let mut folders = HashSet::new();
        let mut stack: Vec<&FolderNode> = tree.children.iter().collect();
        while let Some(node) = stack.pop() {
            // Its own folders, and those in every space (ADR 0030).
            if spaces.holds_folder(&node.path, id) {
                folders.insert(node.path.clone());
            }
            stack.extend(node.children.iter());
        }
        Self { folders }
    }

    /// Whether the document at `path` is in the space (its folder is).
    pub fn holds(&self, path: &str) -> bool {
        path.rsplit_once('/').is_some_and(|(folder, _)| self.folders.contains(folder))
    }

    pub fn holds_folder(&self, folder: &str) -> bool {
        self.folders.contains(folder)
    }
}
