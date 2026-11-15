/// Lists a vault's folders, empty ones too, so the sidebar shows them as Obsidian does.
pub trait FolderLister {
    /// Every visible folder (hidden ones and the attachments folder left out), sorted by path.
    fn folders(&self) -> std::io::Result<Vec<String>>;
}
