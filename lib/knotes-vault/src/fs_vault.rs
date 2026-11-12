use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use knotes_core::{FileKind, VaultFile, VaultReader, VaultScanner};
use rayon::prelude::*;

pub struct FsVault {
    root: PathBuf,
}

impl FsVault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl VaultScanner for FsVault {
    /// Walks folders in parallel. Hidden entries (`.obsidian`, `.git`, `.trash`, dotfiles),
    /// symlinks and names that are not UTF-8 are skipped.
    fn scan(&self) -> io::Result<Vec<VaultFile>> {
        let mut files = scan_dir(&self.root, "")?;
        files.par_sort_unstable_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }
}

fn scan_dir(abs: &Path, rel: &str) -> io::Result<Vec<VaultFile>> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(abs)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| !n.starts_with('.')) else { continue };
        let path = if rel.is_empty() { name.to_owned() } else { format!("{rel}/{name}") };
        // `file_type` and `metadata` of a DirEntry do not follow symlinks.
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            dirs.push((entry.path(), path));
        } else if file_type.is_file() {
            match entry.metadata() {
                Ok(meta) => files.push(VaultFile { kind: FileKind::from_path(&path), path, size: meta.len(), mtime_ns: mtime_ns(&meta) }),
                // Deleted between listing and stat: not part of the vault any more.
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    let nested: Vec<Vec<VaultFile>> = dirs.par_iter().map(|(abs, rel)| scan_dir(abs, rel)).collect::<io::Result<_>>()?;
    files.extend(nested.into_iter().flatten());
    Ok(files)
}

fn mtime_ns(meta: &Metadata) -> i64 {
    meta.mtime() * 1_000_000_000 + meta.mtime_nsec()
}

impl VaultReader for FsVault {
    fn read_text(&self, path: &str) -> io::Result<String> {
        let bytes = fs::read(self.root.join(path))?;
        Ok(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }
}
