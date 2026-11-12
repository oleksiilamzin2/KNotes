//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod completion;
mod document;
mod highlight;
mod restyle;
mod subpath;
mod utf16;

pub use completion::{LinkCompletion, link_completion};
pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{Highlight, LinkRun, Run, Style, highlight};
pub use restyle::{Edit, changed_region};
pub use subpath::{note_headings, subpath_offset};
pub use utf16::{byte_offset, utf16_offsets};
