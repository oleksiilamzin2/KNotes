//! Read-only tools over a Knotes vault (ADR 0027): search, read a note's blocks or a PDF's
//! pages, outlines, links, notes and tags. The in-app chat gives them to the model as function
//! tools; the MCP server gives them to external agents.

mod specs;
mod tool_error;
mod tool_index;
mod vault_tools;

pub use specs::{ToolSpec, specs};
pub use tool_error::ToolError;
pub use tool_index::ToolIndex;
pub use vault_tools::VaultTools;
