//! In-memory search engines.

mod regex_scanner;
mod title_index;

pub use regex_scanner::RegexScanner;
pub use title_index::TitleIndex;
