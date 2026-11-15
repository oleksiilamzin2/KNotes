use knotes_spaces::{Space, Spaces};

/// The space whose mark (its coloured dot) a file shows (ADR 0030): in the whole vault, the
/// space it is in; in a space, only another space (its own space's files go unmarked).
pub fn space_mark<'a>(spaces: &'a Spaces, current: Option<&str>, path: &str) -> Option<&'a Space> {
    spaces.of(path).filter(|space| current != Some(space.id.as_str()))
}

/// The space whose mark a folder's row shows: the space its own rules set (its subfolders, which
/// take it, go unmarked), unless it is the window's (a folder under «Решта бази» shows its).
pub fn folder_space_mark<'a>(spaces: &'a Spaces, current: Option<&str>, folder: &str) -> Option<&'a Space> {
    spaces.own(folder).flatten().filter(|space| current != Some(space.id.as_str()))
}
