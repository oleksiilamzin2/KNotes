use knotes_core::{ContentSearch, LinkGraph, NoteCatalog, NoteOutline, NoteRows, NoteText, TagIndex};

/// What the tools read from the index: search, text, outlines, links, tags, notes.
pub trait ToolIndex: ContentSearch + NoteText + NoteOutline + LinkGraph + TagIndex + NoteRows + NoteCatalog {}

impl<T: ContentSearch + NoteText + NoteOutline + LinkGraph + TagIndex + NoteRows + NoteCatalog> ToolIndex for T {}
