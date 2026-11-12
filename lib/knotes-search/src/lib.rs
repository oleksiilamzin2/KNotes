//! In-memory search engines.

mod regex_scanner;
mod scoped_corpus;
mod title_index;

pub use regex_scanner::RegexScanner;
pub use scoped_corpus::ScopedCorpus;
pub use title_index::TitleIndex;
