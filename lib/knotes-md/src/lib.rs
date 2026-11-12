//! Obsidian-flavoured Markdown parser built on pulldown-cmark.

mod frontmatter;
mod link_target;
mod markdown_parser;
mod tag_scan;

pub use markdown_parser::MarkdownParser;
