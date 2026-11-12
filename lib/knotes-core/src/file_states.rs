use crate::BoxError;

/// What the index knows about a file, compared with a fresh scan to find changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
}

pub trait FileStates {
    /// All indexed files, sorted by path.
    fn file_states(&self) -> Result<Vec<FileState>, BoxError>;
}
