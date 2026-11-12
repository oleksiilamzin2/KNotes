//! Domain model and traits (ports) of Knotes. No I/O lives here.

mod note;
mod note_parser;
mod vault_file;
mod vault_scanner;

pub use note::{Block, BlockAnchor, Frontmatter, Heading, Link, LinkKind, ParsedNote, Span, Tag};
pub use note_parser::NoteParser;
pub use vault_file::{FileKind, VaultFile};
pub use vault_scanner::VaultScanner;
