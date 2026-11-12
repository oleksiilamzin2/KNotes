//! View models of the Knotes window: plain Rust, no AppKit.

mod folder_tree;
mod natural_order;
mod note_list;
mod tag_tree;

pub use folder_tree::{FolderNode, folder_tree};
pub use natural_order::{natural_cmp, natural_key};
pub use note_list::{Clock, DateGroup, ListFilter, ListItem, ListSettings, SortOrder, build_note_list, title_of};
pub use tag_tree::{TagNode, tag_tree};
