//! Domain model and traits (ports) of Knotes. No I/O lives here.

mod content_search;
mod file_states;
mod index_writer;
mod link_graph;
mod note;
mod note_outline;
mod note_parser;
mod note_text;
mod tag_index;
mod vault_file;
mod vault_scanner;

pub use content_search::{BlockHit, ContentSearch};
pub use file_states::{FileState, FileStates};
pub use index_writer::{IndexChange, IndexWriter};
pub use link_graph::LinkGraph;
pub use note::{Block, BlockAnchor, Frontmatter, Heading, Link, LinkKind, ParsedNote, Span, Tag};
pub use note_outline::NoteOutline;
pub use note_parser::NoteParser;
pub use note_text::NoteText;
pub use tag_index::TagIndex;
pub use vault_file::{FileKind, VaultFile};
pub use vault_scanner::VaultScanner;

/// Error type of the ports; adapters box their own errors into it.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
