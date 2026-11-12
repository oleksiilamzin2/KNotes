use std::collections::HashMap;
use std::sync::Arc;

use knotes_app::{FolderNode, FolderRules, TagNode, folder_tree, tag_tree};
use knotes_core::{BoxError, CatalogEntry, FolderRulesStore, NoteCatalog, NoteRow, NoteRows, TagIndex};

/// Everything the window shows about a vault at one moment; immutable and cheap to share.
pub struct Snapshot {
    pub rows: Arc<Vec<NoteRow>>,
    pub catalog: Arc<Vec<CatalogEntry>>,
    pub folders: Arc<FolderNode>,
    pub tags: Arc<Vec<TagNode>>,
    /// `.knotes` rules by folder (`""` is the vault root); unreadable files are skipped.
    pub rules: Arc<HashMap<String, FolderRules>>,
    /// Vault paths changed since the previous snapshot (empty for the first ones).
    pub changed: Vec<String>,
}

impl Snapshot {
    pub fn read(
        index: &(impl NoteRows + NoteCatalog + TagIndex),
        rules_store: &impl FolderRulesStore,
        changed: Vec<String>,
    ) -> Result<Self, BoxError> {
        let rows = index.note_rows()?;
        let catalog = index.catalog()?;
        let folders = folder_tree(catalog.iter().map(|e| e.path.as_str()));
        let tags = tag_tree(&index.tag_counts()?);
        let mut paths = Vec::new();
        collect_paths(&folders, &mut paths);
        let rules = rules_store
            .read_rules(&paths.iter().map(String::as_str).collect::<Vec<_>>())?
            .into_iter()
            .filter_map(|(folder, text)| match FolderRules::parse(&text) {
                Ok(rules) => Some((folder, rules)),
                Err(e) => {
                    eprintln!("knotes: ignoring {folder}/.knotes: {e}");
                    None
                }
            })
            .collect();
        Ok(Self {
            rows: Arc::new(rows),
            catalog: Arc::new(catalog),
            folders: Arc::new(folders),
            tags: Arc::new(tags),
            rules: Arc::new(rules),
            changed,
        })
    }
}

fn collect_paths(node: &FolderNode, out: &mut Vec<String>) {
    out.push(node.path.clone());
    for child in &node.children {
        collect_paths(child, out);
    }
}
