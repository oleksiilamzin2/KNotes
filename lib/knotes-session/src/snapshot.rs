use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use knotes_app::{FolderNode, FolderRules, SpaceFolders, SpaceTags, TagNode, count_tags, folder_tree_with, tag_tree};
use knotes_core::{BoxError, CatalogEntry, FolderLister, FolderRulesStore, NoteCatalog, NoteRow, NoteRows, TagIndex, TrashEntry, TrashStore};
use knotes_index::LinkTargets;
use knotes_search::TitleIndex;
use knotes_spaces::Spaces;

/// Everything the window shows about a vault at one moment; immutable and cheap to share.
#[derive(Clone)]
pub struct Snapshot {
    pub rows: Arc<Vec<NoteRow>>,
    pub catalog: Arc<Vec<CatalogEntry>>,
    pub folders: Arc<FolderNode>,
    /// Every folder of the vault, empty ones too: listed again only when a change is not a
    /// known file's (a folder came or went).
    pub folder_paths: Arc<Vec<String>>,
    pub tags: Arc<Vec<TagNode>>,
    /// Every tag a note has with the note, a note's together: a space's tags count from them
    /// (ADR 0030). Kept from the previous snapshot while the same, with `tags`.
    pub tagged: Arc<Vec<(String, String)>>,
    /// Notes without any tag; kept from the previous snapshot while the same.
    pub untagged: Arc<HashSet<String>>,
    /// `.knotes.json` rules by folder (`""` is the vault root); unreadable files are skipped.
    pub rules: Arc<HashMap<String, FolderRules>>,
    /// The vault's spaces and its folders' (ADR 0030), from the rules.
    pub spaces: Arc<Spaces>,
    /// Each space's tags, those only outside it and its untagged notes, in the order of
    /// `spaces`: counted here, off the main thread, and only when they may have changed.
    pub space_tags: Arc<Vec<SpaceTags>>,
    /// The recently deleted, the newest first (ADR 0032); only new entries read, the list kept
    /// from the previous snapshot while the same.
    pub trash: Arc<Vec<TrashEntry>>,
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
        rules_store: &(impl FolderRulesStore + FolderLister + TrashStore),
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
        let known = |path: &str| catalog.binary_search_by(|e| e.path.as_str().cmp(path)).is_ok() || previous.is_some_and(|p| p.catalog.binary_search_by(|e| e.path.as_str().cmp(path)).is_ok());
        let folder_paths = match previous.filter(|_| changed.iter().all(|path| known(path))) {
            Some(previous) => Arc::clone(&previous.folder_paths),
            None => Arc::new(rules_store.folders()?),
        };
        let folders = folder_tree_with(catalog.iter().map(|e| e.path.as_str()), folder_paths.iter().map(String::as_str));
        // The tags count from every note's (no query of their own), only when they changed.
        let tagged = index.tagged_notes()?;
        let (tagged, tags) = match previous.filter(|p| *p.tagged == tagged) {
            Some(p) => (Arc::clone(&p.tagged), Arc::clone(&p.tags)),
            None => {
                let tags = tag_tree(&count_tags(tagged.iter().map(|(tag, path)| (tag.as_str(), path.as_str()))));
                (Arc::new(tagged), Arc::new(tags))
            }
        };
        let untagged: HashSet<String> = index.untagged_notes()?.into_iter().collect();
        let untagged = match previous.filter(|p| *p.untagged == untagged) {
            Some(p) => Arc::clone(&p.untagged),
            None => Arc::new(untagged),
        };
        let mut paths = Vec::new();
        collect_paths(&folders, &mut paths);
        let rules = rules_store
            .read_rules(&paths.iter().map(String::as_str).collect::<Vec<_>>())?
            .into_iter()
            .filter_map(|(folder, text)| match FolderRules::parse(&text) {
                Ok(rules) => Some((folder, rules)),
                Err(e) => {
                    eprintln!("knotes: ignoring {folder}/.knotes.json: {e}");
                    None
                }
            })
            .collect::<HashMap<String, FolderRules>>();
        let spaces = Spaces::from_rules(rules.iter().map(|(folder, rules)| (folder.as_str(), rules.object())));
        let space_folders: Vec<SpaceFolders> = spaces.list().iter().map(|space| SpaceFolders::new(&folders, &spaces, &space.id)).collect();
        let same = |p: &&Snapshot| {
            Arc::ptr_eq(&p.tagged, &tagged)
                && Arc::ptr_eq(&p.untagged, &untagged)
                && p.space_tags.len() == space_folders.len()
                && p.space_tags.iter().zip(spaces.list()).zip(&space_folders).all(|((kept, space), folders)| kept.id == space.id && kept.folders == *folders)
        };
        let space_tags = match previous.filter(same) {
            Some(p) => Arc::clone(&p.space_tags),
            None => Arc::new(spaces.list().iter().zip(space_folders).map(|(space, folders)| SpaceTags::count(&space.id, folders, &tagged, &untagged)).collect()),
        };
        let trash = rules_store.trashed_with(previous.map_or(&[], |p| p.trash.as_slice())).unwrap_or_default();
        let trash = match previous.filter(|p| *p.trash == trash) {
            Some(p) => Arc::clone(&p.trash),
            None => Arc::new(trash),
        };
        Ok(Self {
            rows: Arc::new(rows),
            catalog,
            links,
            titles,
            folders: Arc::new(folders),
            folder_paths,
            tags,
            tagged,
            untagged,
            rules: Arc::new(rules),
            spaces: Arc::new(spaces),
            space_tags,
            trash,
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
