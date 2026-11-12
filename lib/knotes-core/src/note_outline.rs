use crate::{BoxError, Heading};

/// Headings of an indexed note, in source order.
pub trait NoteOutline {
    fn headings(&self, path: &str) -> Result<Vec<Heading>, BoxError>;
}
