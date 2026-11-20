//! Editor model: what the live preview styles and which markup it can hide (ADR 0005, 0011).

mod canvas;
mod code_highlight;
mod completion;
mod document;
mod formula;
mod highlight;
mod image_width;
mod note_tag;
mod restyle;
mod subpath;
mod table_edit;
mod table_typing;
mod tag_completion;
mod task_box;
mod utf16;

pub use canvas::Canvas;
pub use code_highlight::{CodeHighlighter, CodeToken, highlight_code, load_code_grammars};
pub use completion::{LinkCompletion, link_completion};
pub use formula::{FormulaCache, FormulaImage, FormulaStyle, render_formula};
pub use document::{DiskChange, Document, conflict_path};
pub use highlight::{Align, CalloutRun, CodeRun, FootnoteRun, FormulaRun, Highlight, ImageRun, LinkRun, ListKind, NoteEmbedRun, Run, Style, TableRow, TableRun, highlight, reaches};
pub use image_width::set_image_width;
pub use note_tag::{with_tag, without_tag};
pub use restyle::{Edit, changed_region};
pub use subpath::{embedded_part, note_headings, splice_part, subpath_offset};
pub use table_typing::{TableTyping, cell_at, cell_text, nearest_cell_edge, table_typing};
pub use table_edit::TableText;
pub use tag_completion::{TagCompletion, tag_completion, tag_typed};
pub use task_box::task_box_typed;
pub use utf16::{byte_offset, utf16_offsets};
