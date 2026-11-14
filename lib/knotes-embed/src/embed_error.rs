use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// The model cannot be run here: the helper or the model file is missing, or will not start.
    Unavailable(String),
    /// The model was run and failed, or its answer made no sense.
    Failed(String),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "search by meaning is unavailable: {why}"),
            Self::Failed(why) => write!(f, "search by meaning failed: {why}"),
        }
    }
}

impl std::error::Error for EmbedError {}
