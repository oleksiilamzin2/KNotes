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
    /// Empty space before the rest of the vault's row, in a space (ADR 0030).
    RestGap,
    Folder(String),
    Tag(String),
    /// A document open as a tab (ADR 0024), by its vault path.
    Tab(String),
    /// A deck of cards (ADR 0037), by its path.
    Deck(String),
    /// The rest of the vault, in a space (ADR 0030): its notes, and its folders or tags below.
    Rest,
    /// The recently deleted (ADR 0032), and the empty space before them.
    Deleted,
    DeletedGap,
}

impl SidebarItem {
    /// Stable text key: the outline view keeps one object per key, so expansion and
    /// selection survive new snapshots.
    pub fn key(&self) -> String {
        match self {
            Self::AllNotes => "all".into(),
            Self::Untagged => "untagged".into(),
            Self::Gap => "gap".into(),
            Self::RestGap => "restgap".into(),
            Self::Folder(path) => format!("f:{path}"),
            Self::Tag(tag) => format!("t:{tag}"),
            Self::Tab(path) => format!("tab:{path}"),
            Self::Deck(deck) => format!("d:{deck}"),
            Self::Rest => "rest".into(),
            Self::Deleted => "deleted".into(),
            Self::DeletedGap => "deletedgap".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "all" => Some(Self::AllNotes),
            "untagged" => Some(Self::Untagged),
            "gap" => Some(Self::Gap),
            "restgap" => Some(Self::RestGap),
            "rest" => Some(Self::Rest),
            "deleted" => Some(Self::Deleted),
            "deletedgap" => Some(Self::DeletedGap),
            _ => match key.split_once(':')? {
                ("f", path) => Some(Self::Folder(path.into())),
                ("t", tag) => Some(Self::Tag(tag.into())),
                ("tab", path) => Some(Self::Tab(path.into())),
                ("d", deck) => Some(Self::Deck(deck.into())),
                _ => None,
            },
        }
    }
}

/// What the sidebar lists: the folder tree or the tag tree under "All notes", or the tabs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    #[default]
    Folders,
    Tags,
    Tabs,
    /// The decks of the notes' cards (ADR 0037).
    Cards,
}

impl SidebarMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Folders => "folders",
            Self::Tags => "tags",
            Self::Tabs => "tabs",
            Self::Cards => "cards",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "folders" => Some(Self::Folders),
            "tags" => Some(Self::Tags),
            "tabs" => Some(Self::Tabs),
            "cards" => Some(Self::Cards),
            _ => None,
        }
    }
}

/// What the sidebar lists of the rest of the vault in a space (ADR 0030): its folders and
/// tags (as the space's split gives them) and how many notes it has.
pub struct RestPart<'a> {
    pub folders: &'a FolderNode,
    pub tags: &'a [TagNode],
    pub notes: usize,
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
    /// `with_subfolders` says whether a folder's list shows its subfolders' notes: its count
    /// counts what its list shows.
    /// `rest`: in a space, the rest of the vault, under a row of its own after the space's tree.
    /// `deleted`: how many entries the recently deleted have (a row last, while there is any).
    #[allow(clippy::too_many_arguments)]
    pub fn new(folders: &FolderNode, tags: &[TagNode], mode: SidebarMode, inbox: Option<&str>, untagged: usize, with_subfolders: &dyn Fn(&str) -> bool, rest: Option<RestPart>, deleted: usize) -> Self {
        let mut entries = HashMap::new();
        let fixed = |count| Entry { name: String::new(), count, children: Vec::new() };
        entries.insert(SidebarItem::AllNotes, fixed(Some(folders.notes)));
        entries.insert(SidebarItem::Untagged, fixed(Some(untagged)));
        entries.insert(SidebarItem::Gap, fixed(None));
        let inbox = inbox.filter(|p| !p.is_empty());
        let mut top = vec![SidebarItem::AllNotes];
        let inbox_item = inbox.map(|path| {
            let node = find_folder(folders, path);
            let children = node.map(|n| n.children.iter().map(|c| add_folder(c, &mut entries, path, with_subfolders)).collect()).unwrap_or_default();
            let name = node.map_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned(), |n| n.name.clone());
            let item = SidebarItem::Folder(path.to_owned());
            entries.insert(item.clone(), Entry { name, count: Some(node.map_or(0, |n| shown_notes(n, with_subfolders))), children });
            item
        });
        top.extend(inbox_item.clone());
        top.extend([SidebarItem::Untagged, SidebarItem::Gap]);
        let skip = inbox.unwrap_or("");
        match mode {
            SidebarMode::Folders => {
                top.extend(folders.children.iter().filter(|f| f.path != skip).map(|f| add_folder(f, &mut entries, skip, with_subfolders)));
            }
            SidebarMode::Tags => top.extend(tags.iter().map(|t| add_tag(t, &mut entries))),
            SidebarMode::Tabs => return Self::tabs(&[]),
            SidebarMode::Cards => return Self::decks(&[]),
        }
        if let Some(rest) = rest {
            let children = match mode {
                SidebarMode::Tags => rest.tags.iter().map(|t| add_tag(t, &mut entries)).collect(),
                _ => rest.folders.children.iter().filter(|f| f.path != skip).map(|f| add_folder(f, &mut entries, skip, with_subfolders)).collect(),
            };
            entries.insert(SidebarItem::Rest, Entry { name: String::new(), count: Some(rest.notes), children });
            entries.insert(SidebarItem::RestGap, fixed(None));
            top.extend([SidebarItem::RestGap, SidebarItem::Rest]);
        }
        if deleted > 0 {
            entries.insert(SidebarItem::DeletedGap, fixed(None));
            entries.insert(SidebarItem::Deleted, fixed(Some(deleted)));
            top.extend([SidebarItem::DeletedGap, SidebarItem::Deleted]);
        }
        Self { top, entries, inbox: inbox_item }
    }

    /// The tabs (ADR 0024): a row per document, each its path and the name it shows.
    pub fn tabs(tabs: &[(String, String)]) -> Self {
        let mut entries = HashMap::new();
        let top = tabs
            .iter()
            .map(|(path, name)| {
                let item = SidebarItem::Tab(path.clone());
                entries.insert(item.clone(), Entry { name: name.clone(), count: None, children: Vec::new() });
                item
            })
            .collect();
        Self { top, entries, inbox: None }
    }

    /// The decks (ADR 0037): a tree as `deck_tree` gives it, each with its cards to review.
    pub fn decks(decks: &[TagNode]) -> Self {
        fn add(node: &TagNode, entries: &mut HashMap<SidebarItem, Entry>) -> SidebarItem {
            let children = node.children.iter().map(|c| add(c, entries)).collect();
            let item = SidebarItem::Deck(node.tag.clone());
            entries.insert(item.clone(), Entry { name: node.name.clone(), count: Some(node.notes), children });
            item
        }
        let mut entries = HashMap::new();
        let top = decks.iter().map(|deck| add(deck, &mut entries)).collect();
        Self { top, entries, inbox: None }
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
fn add_folder(node: &FolderNode, entries: &mut HashMap<SidebarItem, Entry>, skip: &str, with_subfolders: &dyn Fn(&str) -> bool) -> SidebarItem {
    let children = node.children.iter().filter(|c| c.path != skip).map(|c| add_folder(c, entries, skip, with_subfolders)).collect();
    let item = SidebarItem::Folder(node.path.clone());
    entries.insert(item.clone(), Entry { name: node.name.clone(), count: Some(shown_notes(node, with_subfolders)), children });
    item
}

/// The notes a folder's list shows: all below it, or only its own.
fn shown_notes(node: &FolderNode, with_subfolders: &dyn Fn(&str) -> bool) -> usize {
    if with_subfolders(&node.path) { node.notes } else { node.notes - node.children.iter().map(|c| c.notes).sum::<usize>() }
}

fn add_tag(node: &TagNode, entries: &mut HashMap<SidebarItem, Entry>) -> SidebarItem {
    let children = node.children.iter().map(|c| add_tag(c, entries)).collect();
    let item = SidebarItem::Tag(node.tag.clone());
    entries.insert(item.clone(), Entry { name: node.name.clone(), count: Some(node.notes), children });
    item
}
