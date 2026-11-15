use crate::SpaceFolders;

/// How a list keeps its notes when the window works in a space (ADR 0030).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceList<'a> {
    /// The space's notes, then the rest of the vault's under a «Решта бази» header.
    Space(&'a SpaceFolders),
    /// The notes outside the space only (what the sidebar's «Решта бази» rows list).
    Rest(&'a SpaceFolders),
}

/// Which notes a part of a list keeps: all, the space's, or those outside it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Side<'a> {
    All,
    In(&'a SpaceFolders),
    Out(&'a SpaceFolders),
}

impl Side<'_> {
    pub(crate) fn keeps(self, path: &str) -> bool {
        match self {
            Side::All => true,
            Side::In(space) => space.holds(path),
            Side::Out(space) => !space.holds(path),
        }
    }
}
