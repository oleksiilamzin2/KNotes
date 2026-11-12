use crate::ParsedNote;

/// Turns the source of one note into its indexable structure.
pub trait NoteParser {
    fn parse(&self, source: &str) -> ParsedNote;
}
