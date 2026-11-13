//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod canvas;
mod code_highlight;
mod completion;
mod document;
mod formula;
mod highlight;
mod image_width;
mod restyle;
mod subpath;
mod table_edit;
mod task_box;
mod utf16;

pub use canvas::Canvas;
pub use code_highlight::{CodeHighlighter, CodeToken, highlight_code, load_code_grammars};
pub use completion::{LinkCompletion, link_completion};
pub use formula::{FormulaCache, FormulaImage, FormulaStyle, render_formula};
pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{Align, CalloutRun, CodeRun, FormulaRun, Highlight, ImageRun, LinkRun, ListKind, Run, Style, TableRow, TableRun, highlight};
pub use image_width::set_image_width;
pub use restyle::{Edit, changed_region};
pub use subpath::{note_headings, subpath_offset};
pub use table_edit::TableText;
pub use task_box::task_box_typed;
pub use utf16::{byte_offset, utf16_offsets};
