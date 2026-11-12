use std::collections::HashMap;

use crate::{FolderNode, TagNode};

/// A row of the sidebar. Fixed rows have no name of their own; the UI gives them localized
/// labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SidebarItem {
    AllNotes,
    Folder(String),
    Tag(String),
}

impl SidebarItem {
    /// Stable text key: the outline view keeps one object per key, so expansion and
    /// selection survive new snapshots.
    pub fn key(&self) -> String {
        match self {
            Self::AllNotes => "all".into(),
            Self::Folder(path) => format!("f:{path}"),
            Self::Tag(tag) => format!("t:{tag}"),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "all" => Some(Self::AllNotes),
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

/// Sidebar contents for one snapshot: "All notes", then the folder tree or the tag tree, with
/// no heading (the toolbar switch says which).
pub struct Sidebar {
    top: Vec<SidebarItem>,
    entries: HashMap<SidebarItem, Entry>,
}

impl Sidebar {
    pub fn new(folders: &FolderNode, tags: &[TagNode], mode: SidebarMode) -> Self {
        let mut entries = HashMap::new();
        entries.insert(SidebarItem::AllNotes, Entry { name: String::new(), count: Some(folders.notes), children: Vec::new() });
        let mut top = vec![SidebarItem::AllNotes];
        match mode {
            SidebarMode::Folders => top.extend(folders.children.iter().map(|f| add_folder(f, &mut entries))),
            SidebarMode::Tags => top.extend(tags.iter().map(|t| add_tag(t, &mut entries))),
        }
        Self { top, entries }
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

fn add_folder(node: &FolderNode, entries: &mut HashMap<SidebarItem, Entry>) -> SidebarItem {
    let children = node.children.iter().map(|c| add_folder(c, entries)).collect();
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
