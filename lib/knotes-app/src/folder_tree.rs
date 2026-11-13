use std::collections::HashMap;

use knotes_core::FileKind;

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
