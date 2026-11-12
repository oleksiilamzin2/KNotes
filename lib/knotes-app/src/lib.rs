//! View models of the Knotes window: plain Rust, no AppKit.

mod folder_tree;
mod natural_order;
mod tag_tree;

pub use folder_tree::{FolderNode, folder_tree};
pub use natural_order::natural_cmp;
pub use tag_tree::{TagNode, tag_tree};
