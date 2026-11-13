//! View models of the Knotes window: plain Rust, no AppKit.

mod find_in_text;
mod folder_rules;
mod folder_tree;
mod history;
mod image_embed;
mod labels;
mod link_rewrite;
mod natural_order;
mod search_list;
mod search_query;
mod new_note;
mod note_list;
mod outline;
mod panel_mode;
mod relink_plan;
mod sidebar;
mod tabs;
mod tag_tree;

pub use find_in_text::find_all;
pub use folder_rules::{FolderRules, RULES_FILE, new_notes_folder, resolve_list_settings};
pub use folder_tree::{FolderNode, folder_tree};
pub use history::{HISTORY_DEPTH, History};
pub use image_embed::{embed_images, pasted_image_name};
pub use labels::{TimeStyle, plural_uk, time_style};
pub use link_rewrite::{Moves, relink};
pub use natural_order::{natural_cmp, natural_key};
pub use search_list::{ContentMatch, found_snippet, found_words, search_list, search_prefixes};
pub use search_query::{SearchMode, parse_search, with_search_mode};
pub use new_note::{note_file_stem, unique_note_path};
pub use relink_plan::{Relinks, plan_relinks};
pub use note_list::{Clock, DateGroup, ListFilter, ListItem, ListOption, ListSettings, SortOrder, build_note_list, days_from_civil, list_scope, title_of};
pub use outline::{Outline, OutlineEntry, OutlinePlace};
pub use panel_mode::PanelMode;
pub use sidebar::{Sidebar, SidebarItem, SidebarMode};
pub use tabs::{AfterClose, TabPlace, Tabs};
pub use tag_tree::{TagNode, tag_tree};
