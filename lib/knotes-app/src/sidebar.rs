use std::collections::HashMap;

use crate::{FolderNode, TagNode};

/// A row of the sidebar. Fixed rows have no name of their own; the UI gives them localized
/// labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SidebarItem {
    AllNotes,
    /// Notes without any tag.
    Untagged,
    /// Empty space between the fixed rows and the tree.
    Gap,
    Folder(String),
    Tag(String),
}

impl SidebarItem {
    /// Stable text key: the outline view keeps one object per key, so expansion and
    /// selection survive new snapshots.
    pub fn key(&self) -> String {
        match self {
            Self::AllNotes => "all".into(),
            Self::Untagged => "untagged".into(),
            Self::Gap => "gap".into(),
            Self::Folder(path) => format!("f:{path}"),
            Self::Tag(tag) => format!("t:{tag}"),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "all" => Some(Self::AllNotes),
            "untagged" => Some(Self::Untagged),
            "gap" => Some(Self::Gap),
            _ => match key.split_once(':')? {
                ("f", path) => Some(Self::Folder(path.into())),
                ("t", tag) => Some(Self::Tag(tag.into())),
                _ => None,
            },
        }
    }
}

/// What the sidebar lists under "All notes": the folder tree or the tag tree.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    #[default]
    Folders,
    Tags,
}

impl SidebarMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Folders => "folders",
            Self::Tags => "tags",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "folders" => Some(Self::Folders),
            "tags" => Some(Self::Tags),
            _ => None,
        }
    }
}

struct Entry {
    name: String,
    count: Option<usize>,
    children: Vec<SidebarItem>,
}

/// Sidebar contents for one snapshot: "All notes", the inbox (the new notes folder, pinned in
/// both modes), "Untagged", a gap, then the folder tree or the tag tree, with no heading (the
/// toolbar switch says which).
pub struct Sidebar {
    top: Vec<SidebarItem>,
    entries: HashMap<SidebarItem, Entry>,
    inbox: Option<SidebarItem>,
}

impl Sidebar {
    /// `inbox`: path of the new notes folder (`None` or `""`: no inbox row); it shows even
    /// before the folder exists, and the folder tree leaves it out. `untagged`: notes without
    /// any tag.
    pub fn new(folders: &FolderNode, tags: &[TagNode], mode: SidebarMode, inbox: Option<&str>, untagged: usize) -> Self {
        let mut entries = HashMap::new();
        let fixed = |count| Entry { name: String::new(), count, children: Vec::new() };
        entries.insert(SidebarItem::AllNotes, fixed(Some(folders.notes)));
        entries.insert(SidebarItem::Untagged, fixed(Some(untagged)));
        entries.insert(SidebarItem::Gap, fixed(None));
        let inbox = inbox.filter(|p| !p.is_empty());
        let mut top = vec![SidebarItem::AllNotes];
        let inbox_item = inbox.map(|path| {
            let node = find_folder(folders, path);
            let children = node.map(|n| n.children.iter().map(|c| add_folder(c, &mut entries, path)).collect()).unwrap_or_default();
            let name = node.map_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned(), |n| n.name.clone());
            let item = SidebarItem::Folder(path.to_owned());
            entries.insert(item.clone(), Entry { name, count: Some(node.map_or(0, |n| n.notes)), children });
            item
        });
        top.extend(inbox_item.clone());
        top.extend([SidebarItem::Untagged, SidebarItem::Gap]);
        let skip = inbox.unwrap_or("");
        match mode {
            SidebarMode::Folders => {
                top.extend(folders.children.iter().filter(|f| f.path != skip).map(|f| add_folder(f, &mut entries, skip)));
            }
            SidebarMode::Tags => top.extend(tags.iter().map(|t| add_tag(t, &mut entries))),
        }
        Self { top, entries, inbox: inbox_item }
    }

    /// The pinned inbox row, if the vault has a new notes folder.
    pub fn inbox(&self) -> Option<&SidebarItem> {
        self.inbox.as_ref()
    }

    /// Children of `item`, or the top-level rows for `None`.
    pub fn children(&self, item: Option<&SidebarItem>) -> Vec<SidebarItem> {
        match item {
            None => self.top.clone(),
            Some(item) => self.entries.get(item).map(|e| e.children.clone()).unwrap_or_default(),
        }
    }

    pub fn child(&self, item: Option<&SidebarItem>, index: usize) -> Option<&SidebarItem> {
        match item {
            None => self.top.get(index),
            Some(item) => self.entries.get(item)?.children.get(index),
        }
    }

    pub fn child_count(&self, item: Option<&SidebarItem>) -> usize {
        match item {
            None => self.top.len(),
            Some(item) => self.entries.get(item).map_or(0, |e| e.children.len()),
        }
    }

    /// Notes under the item; `None` for section headers.
    pub fn count(&self, item: &SidebarItem) -> Option<usize> {
        self.entries.get(item).and_then(|e| e.count)
    }

    /// Folder or tag name; empty for fixed rows.
    pub fn name(&self, item: &SidebarItem) -> &str {
        self.entries.get(item).map_or("", |e| e.name.as_str())
    }
}

fn find_folder<'a>(node: &'a FolderNode, path: &str) -> Option<&'a FolderNode> {
    if node.path == path {
        return Some(node);
    }
    node.children.iter().find(|c| path == c.path || path.starts_with(&format!("{}/", c.path))).and_then(|c| find_folder(c, path))
}

/// Adds `node` and its subfolders, leaving out the folder at `skip` (the pinned inbox).
fn add_folder(node: &FolderNode, entries: &mut HashMap<SidebarItem, Entry>, skip: &str) -> SidebarItem {
    let children = node.children.iter().filter(|c| c.path != skip).map(|c| add_folder(c, entries, skip)).collect();
    let item = SidebarItem::Folder(node.path.clone());
    entries.insert(item.clone(), Entry { name: node.name.clone(), count: Some(node.notes), children });
    item
}

fn add_tag(node: &TagNode, entries: &mut HashMap<SidebarItem, Entry>) -> SidebarItem {
    let children = node.children.iter().map(|c| add_tag(c, entries)).collect();
    let item = SidebarItem::Tag(node.tag.clone());
    entries.insert(item.clone(), Entry { name: node.name.clone(), count: Some(node.notes), children });
    item
}
