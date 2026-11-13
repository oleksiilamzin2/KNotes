//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod canvas;
mod completion;
mod document;
mod formula;
mod highlight;
mod restyle;
mod subpath;
mod title;
mod task_box;
mod utf16;

pub use canvas::Canvas;
pub use completion::{LinkCompletion, link_completion};
pub use formula::{FormulaCache, FormulaImage, FormulaStyle, render_formula};
pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{FormulaRun, Highlight, LinkRun, ListKind, Run, Style, highlight};
pub use restyle::{Edit, changed_region};
pub use subpath::{note_headings, subpath_offset};
pub use title::replace_title;
pub use task_box::task_box_typed;
pub use utf16::{byte_offset, utf16_offsets};
