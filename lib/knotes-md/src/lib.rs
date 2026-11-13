//! Obsidian-flavoured Markdown parser built on pulldown-cmark.

mod anchor_scan;
mod chunking;
mod frontmatter;
mod link_target;
mod markdown_parser;
mod snippet;
mod tag_scan;
mod url_scan;

pub use markdown_parser::{MarkdownParser, OPTIONS};
pub use snippet::snippet;
