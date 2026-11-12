//! Domain model and traits (ports) of Knotes. No I/O lives here.

mod note;
mod note_parser;

pub use note::{Heading, ParsedNote, Span};
pub use note_parser::NoteParser;
