//! Domain model and traits (ports) of Knotes. No I/O lives here.

mod note;
mod note_parser;

pub use note::{BlockAnchor, Frontmatter, Heading, Link, LinkKind, ParsedNote, Span, Tag};
pub use note_parser::NoteParser;
