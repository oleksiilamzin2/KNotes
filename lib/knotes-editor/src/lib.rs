//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod document;
mod highlight;
mod utf16;

pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{Highlight, Run, Style, highlight};
pub use utf16::utf16_offsets;
