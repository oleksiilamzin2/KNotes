/// What a vault file is, decided by its extension (case-insensitive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Markdown,
    Canvas,
    Pdf,
    Image,
    Other,
}

impl FileKind {
    pub fn from_path(path: &str) -> Self {
        let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("md") => Self::Markdown,
            Some("canvas") => Self::Canvas,
            Some("pdf") => Self::Pdf,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "heic" | "bmp" | "avif") => Self::Image,
            _ => Self::Other,
        }
    }
}

/// A file found in the vault. `path` is relative to the vault root and `/`-separated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFile {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    /// Modification time, nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
}
