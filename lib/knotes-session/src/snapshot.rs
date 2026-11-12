use std::sync::Arc;

use knotes_app::{FolderNode, TagNode, folder_tree, tag_tree};
use knotes_core::{BoxError, CatalogEntry, NoteCatalog, NoteRow, NoteRows, TagIndex};

/// Everything the window shows about a vault at one moment; immutable and cheap to share.
pub struct Snapshot {
    pub rows: Arc<Vec<NoteRow>>,
    pub catalog: Arc<Vec<CatalogEntry>>,
    pub folders: Arc<FolderNode>,
    pub tags: Arc<Vec<TagNode>>,
    /// Vault paths changed since the previous snapshot (empty for the first ones).
    pub changed: Vec<String>,
}

impl Snapshot {
    pub fn read(index: &(impl NoteRows + NoteCatalog + TagIndex), changed: Vec<String>) -> Result<Self, BoxError> {
        let rows = index.note_rows()?;
        let catalog = index.catalog()?;
        let folders = folder_tree(catalog.iter().map(|e| e.path.as_str()));
        let tags = tag_tree(&index.tag_counts()?);
        Ok(Self { rows: Arc::new(rows), catalog: Arc::new(catalog), folders: Arc::new(folders), tags: Arc::new(tags), changed })
    }
}
