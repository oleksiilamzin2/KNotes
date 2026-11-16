//! Obsidian-flavoured Markdown parser built on pulldown-cmark.

mod anchor_scan;
mod chunking;
mod comments;
mod frontmatter;
mod link_target;
mod markdown_parser;
mod snippet;
mod tag_scan;
mod url_scan;

pub use comments::comment_ranges;
pub use markdown_parser::{MarkdownParser, OPTIONS};
pub use snippet::snippet;
