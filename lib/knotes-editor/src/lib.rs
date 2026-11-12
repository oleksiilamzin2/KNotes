//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod document;
mod highlight;
mod restyle;
mod utf16;

pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{Highlight, LinkRun, Run, Style, highlight};
pub use restyle::{Edit, changed_region};
pub use utf16::{byte_offset, utf16_offsets};
