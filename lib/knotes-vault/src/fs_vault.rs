use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use knotes_core::{FileKind, FolderRulesStore, NoteMover, NoteWriter, VaultFile, VaultReader, VaultScanner};
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

/// Name of the per-folder rules file (ADR 0010).
const RULES_FILE: &str = ".knotes";

impl FolderRulesStore for FsVault {
    fn read_rules(&self, folders: &[&str]) -> io::Result<Vec<(String, String)>> {
        let mut found = Vec::new();
        for folder in folders {
            match fs::read_to_string(self.root.join(folder).join(RULES_FILE)) {
                Ok(text) => found.push(((*folder).to_owned(), text)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(found)
    }

    /// Writes a temporary file next to the target and renames it over, so readers (and other
    /// devices syncing the vault) never see half a file.
    fn write_rules(&self, folder: &str, text: &str) -> io::Result<()> {
        let dir = self.root.join(folder);
        let tmp = dir.join(format!("{RULES_FILE}.tmp"));
        fs::write(&tmp, text)?;
        fs::rename(&tmp, dir.join(RULES_FILE)).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

impl FsVault {
    /// The absolute path of a vault-relative path that stays inside the vault.
    fn inside(&self, path: &str) -> io::Result<PathBuf> {
        let rel = Path::new(path);
        let inside = !path.is_empty() && rel.components().all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a path inside the vault: {path}")));
        }
        Ok(self.root.join(rel))
    }
}

impl NoteMover for FsVault {
    fn move_note(&self, from: &str, to: &str) -> io::Result<()> {
        let (source, target) = (self.inside(from)?, self.inside(to)?);
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)?;
        }
        if same_file(&source, &target) {
            // A change of letter case on a case-insensitive volume: the target "exists".
            return fs::rename(&source, &target);
        }
        rename_exclusive(&source, &target)
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// `rename` that fails with `AlreadyExists` instead of replacing the target, atomically.
#[cfg(target_os = "macos")]
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    const RENAME_EXCL: u32 = 0x0000_0004;
    unsafe extern "C" {
        fn renamex_np(from: *const std::ffi::c_char, to: *const std::ffi::c_char, flags: u32) -> i32;
    }
    let (from, to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    // SAFETY: two NUL-terminated paths that outlive the call.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), RENAME_EXCL) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    fs::rename(from, to)
}

impl NoteWriter for FsVault {
    /// Writes a hidden temporary file next to the note (the scanner and the watcher skip it)
    /// and renames it over the note.
    fn write_note(&self, path: &str, text: &str) -> io::Result<()> {
        let target = self.inside(path)?;
        let dir = target.parent().unwrap_or(&self.root);
        fs::create_dir_all(dir)?;
        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("note");
        let tmp = dir.join(format!(".{name}.knotes-tmp"));
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &target).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}
