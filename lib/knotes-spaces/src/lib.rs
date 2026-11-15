//! Spaces (ADR 0030): named, coloured sets of a vault's folders, read from its rules files.

mod folder_space;
mod rules_text;
mod space;
mod space_color;
mod spaces;

pub use folder_space::FolderSpace;
pub use rules_text::{new_space_id, set_folder_space, set_spaces};
pub use space::Space;
pub use space_color::SpaceColor;
pub use spaces::{SHARED, Spaces};
