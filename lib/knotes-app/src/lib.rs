//! View models of the Knotes window: plain Rust, no AppKit.

mod folder_rules;
mod folder_tree;
mod history;
mod labels;
mod natural_order;
mod note_list;
mod sidebar;
mod tag_tree;

pub use folder_rules::{FolderRules, RULES_FILE, new_notes_folder, resolve_list_settings};
pub use folder_tree::{FolderNode, folder_tree};
pub use history::{HISTORY_DEPTH, History};
pub use labels::{TimeStyle, plural_uk, time_style};
pub use natural_order::{natural_cmp, natural_key};
pub use note_list::{Clock, DateGroup, ListFilter, ListItem, ListOption, ListSettings, SortOrder, build_note_list, days_from_civil, title_of};
pub use sidebar::{Sidebar, SidebarItem};
pub use tag_tree::{TagNode, tag_tree};
