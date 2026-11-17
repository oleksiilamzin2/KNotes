use knotes_core::{CardIndex, ContentSearch, LinkGraph, NoteCatalog, NoteOutline, NoteRows, NoteText, TagIndex};

/// What the tools read from the index: search, text, outlines, links, tags, notes, cards.
pub trait ToolIndex: ContentSearch + NoteText + NoteOutline + LinkGraph + TagIndex + NoteRows + NoteCatalog + CardIndex {}

impl<T: ContentSearch + NoteText + NoteOutline + LinkGraph + TagIndex + NoteRows + NoteCatalog + CardIndex> ToolIndex for T {}
