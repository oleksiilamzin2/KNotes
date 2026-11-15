use std::collections::HashMap;

use knotes_core::FileKind;

use crate::SpaceFolders;
use crate::natural_order::natural_cmp;

/// A folder of the vault; the root has an empty path and name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderNode {
    pub path: String,
    pub name: String,
    /// Markdown notes in this folder and all folders below it.
    pub notes: usize,
    /// Sorted naturally by name.
    pub children: Vec<FolderNode>,
}

impl FolderNode {
    /// The two parts of the folder tree in a space (ADR 0030): its folders, then the rest of
    /// the vault's, each folder in one part only. A folder of one part inside a folder of the
    /// other hangs from its nearest ancestor in its own part (named by its path from there), or
    /// at the top (named by its whole path); each part counts its own notes (the root's go to
    /// the rest).
    pub fn split(&self, space: &SpaceFolders) -> (FolderNode, FolderNode) {
        let (mut ins, mut outs) = (Vec::new(), Vec::new());
        for child in &self.children {
            let (node, other) = part(child, space);
            if space.holds_folder(&child.path) {
                ins.push(node);
                outs.extend(other);
            } else {
                outs.push(node);
                ins.extend(other);
            }
        }
        let own = self.notes - self.children.iter().map(|c| c.notes).sum::<usize>();
        let root = |mut children: Vec<FolderNode>, own: usize| {
            children.sort_by(|x, y| natural_cmp(&x.name, &y.name));
            FolderNode { path: self.path.clone(), name: self.name.clone(), notes: own + children.iter().map(|c| c.notes).sum::<usize>(), children }
        };
        (root(ins, 0), root(outs, own))
    }
}

/// `node` with the folders below it of its own part of a space's split, and the folders of the
/// other part found below it (each with its own below it), named by their whole path.
fn part(node: &FolderNode, space: &SpaceFolders) -> (FolderNode, Vec<FolderNode>) {
    let mine = space.holds_folder(&node.path);
    let (mut children, mut other) = (Vec::new(), Vec::new());
    for child in &node.children {
        let (child_node, child_other) = part(child, space);
        if space.holds_folder(&child.path) == mine {
            children.push(child_node);
            other.extend(child_other);
        } else {
            other.push(FolderNode { name: child.path.clone(), ..child_node });
            // Below a folder of the other part, back in this one: they hang from here.
            children.extend(child_other.into_iter().map(|g| FolderNode { name: g.path[node.path.len() + 1..].to_owned(), ..g }));
        }
    }
    children.sort_by(|x, y| natural_cmp(&x.name, &y.name));
    let own = node.notes - node.children.iter().map(|c| c.notes).sum::<usize>();
    let notes = own + children.iter().map(|c| c.notes).sum::<usize>();
    (FolderNode { path: node.path.clone(), name: node.name.clone(), notes, children }, other)
}

/// Builds the folder tree from vault-relative file paths. Every folder that holds any file
/// appears; only Markdown notes are counted.
pub fn folder_tree<'a>(paths: impl IntoIterator<Item = &'a str>) -> FolderNode {
    struct Builder {
        path: String,
        notes: usize,
        children: Vec<usize>,
    }
    let mut nodes = vec![Builder { path: String::new(), notes: 0, children: Vec::new() }];
    let mut by_path: HashMap<String, usize> = HashMap::new();
    for path in paths {
        let is_note = FileKind::from_path(path) == FileKind::Markdown;
        let mut current = 0;
        if is_note {
            nodes[0].notes += 1;
        }
        let Some((folders, _file)) = path.rsplit_once('/') else { continue };
        // The attachments folder (ADR 0020) is the only hidden one in the catalog.
        if folders.starts_with('.') {
            continue;
        }
        let mut end = 0;
        for segment in folders.split('/') {
            end += segment.len();
            let folder = &folders[..end];
            end += 1;
            let index = match by_path.get(folder) {
                Some(&i) => i,
                None => {
                    nodes.push(Builder { path: folder.to_owned(), notes: 0, children: Vec::new() });
                    let i = nodes.len() - 1;
                    nodes[current].children.push(i);
                    by_path.insert(folder.to_owned(), i);
                    i
                }
            };
            if is_note {
                nodes[index].notes += 1;
            }
            current = index;
        }
    }

    fn assemble(nodes: &[Builder], i: usize) -> FolderNode {
        let b = &nodes[i];
        let mut children: Vec<FolderNode> = b.children.iter().map(|&c| assemble(nodes, c)).collect();
        children.sort_by(|x, y| natural_cmp(&x.name, &y.name));
        let name = b.path.rsplit('/').next().unwrap_or("").to_owned();
        FolderNode { path: b.path.clone(), name, notes: b.notes, children }
    }
    assemble(&nodes, 0)
}
