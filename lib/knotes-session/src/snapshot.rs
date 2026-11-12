use std::collections::HashMap;
use std::sync::Arc;

use knotes_app::{FolderNode, FolderRules, TagNode, folder_tree, tag_tree};
use knotes_core::{BoxError, CatalogEntry, FolderRulesStore, NoteCatalog, NoteRow, NoteRows, TagIndex};
use knotes_index::LinkTargets;
use knotes_search::TitleIndex;

/// Everything the window shows about a vault at one moment; immutable and cheap to share.
#[derive(Clone)]
pub struct Snapshot {
    pub rows: Arc<Vec<NoteRow>>,
    pub catalog: Arc<Vec<CatalogEntry>>,
    pub folders: Arc<FolderNode>,
    pub tags: Arc<Vec<TagNode>>,
    /// Notes without any tag.
    pub untagged: usize,
    /// `.knotes` rules by folder (`""` is the vault root); unreadable files are skipped.
    pub rules: Arc<HashMap<String, FolderRules>>,
    /// Link resolution for the editor (ADR 0013).
    pub links: Arc<LinkTargets>,
    /// Fuzzy title search, for link completion.
    pub titles: Arc<TitleIndex>,
    /// Vault paths changed since the previous snapshot (empty for the first ones).
    pub changed: Vec<String>,
}

impl Snapshot {
    /// `previous`: its lookups are reused when the files (paths and aliases) are the same.
    pub fn read(
        index: &(impl NoteRows + NoteCatalog + TagIndex),
        rules_store: &impl FolderRulesStore,
        changed: Vec<String>,
        previous: Option<&Snapshot>,
    ) -> Result<Self, BoxError> {
        let rows = index.note_rows()?;
        let catalog = index.catalog()?;
        let (catalog, links, titles) = match previous.filter(|p| *p.catalog == catalog) {
            Some(p) => (Arc::clone(&p.catalog), Arc::clone(&p.links), Arc::clone(&p.titles)),
            None => {
                let links = LinkTargets::new(catalog.iter().map(|e| e.path.as_str()));
                let titles = TitleIndex::new(catalog.iter().map(|e| (e.path.clone(), e.aliases.clone())));
                (Arc::new(catalog), Arc::new(links), Arc::new(titles))
            }
        };
        let folders = folder_tree(catalog.iter().map(|e| e.path.as_str()));
        let tags = tag_tree(&index.tag_counts()?);
        let untagged = index.untagged_notes()?.len();
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
            catalog,
            links,
            titles,
            folders: Arc::new(folders),
            tags: Arc::new(tags),
            untagged,
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
