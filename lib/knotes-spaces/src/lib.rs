//! Spaces (ADR 0030): named, coloured sets of a vault's folders, read from its rules files; and
//! what the user keeps from the AI (ADR 0036), read from the same files.

mod folder_space;
mod kept_from_ai;
mod rules_text;
mod space;
mod space_color;
mod spaces;

pub use folder_space::FolderSpace;
pub use kept_from_ai::{KeptFromAi, NO_AI_TAG};
pub use rules_text::{new_space_id, set_file_ai, set_folder_ai, set_folder_space, set_spaces};
pub use space::Space;
pub use space_color::SpaceColor;
pub use spaces::{SHARED, Spaces};
