/// What a folder's rules say of its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSpace<'a> {
    /// Nothing: the folder takes its parent's.
    Inherit,
    /// None, whatever its parent's (`"space": null`).
    None,
    /// The space with this id.
    In(&'a str),
    /// Every space (`"space": "*"`): a folder each of them shows, as contacts or an inbox.
    Shared,
}
