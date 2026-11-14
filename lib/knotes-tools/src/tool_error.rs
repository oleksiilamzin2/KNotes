use std::fmt;

/// Why a tool call gave no result; told to the model (or the MCP client) as the call's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// No tool of that name.
    Unknown(String),
    /// The arguments do not fit the tool's schema.
    BadArguments(String),
    /// No such document in the vault.
    NotFound(String),
    /// The index or a file could not be read.
    Failed(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "no tool named {name}"),
            Self::BadArguments(why) => write!(f, "bad arguments: {why}"),
            Self::NotFound(path) => write!(f, "no document {path} in the vault"),
            Self::Failed(why) => write!(f, "failed: {why}"),
        }
    }
}

impl std::error::Error for ToolError {}
