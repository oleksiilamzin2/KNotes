//! Obsidian-flavoured Markdown parser built on pulldown-cmark.

mod anchor_scan;
mod chunking;
mod frontmatter;
mod link_target;
mod markdown_parser;
mod snippet;
mod tag_scan;
mod title;

pub use markdown_parser::{MarkdownParser, OPTIONS};
pub use snippet::snippet;
pub use title::{plain_title, title_range};
