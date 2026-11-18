use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use knotes_core::{ATTACHMENTS_FOLDER, AttachmentStore, FileKind, FolderLister, FolderRulesStore, NoteMover, NoteWriter, VaultFile, VaultReader, VaultScanner};
use rayon::prelude::*;

pub struct FsVault {
    pub(crate) root: PathBuf,
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
    /// symlinks and names that are not UTF-8 are skipped, except the attachments folder.
    fn scan(&self) -> io::Result<Vec<VaultFile>> {
        let mut files = scan_dir(&self.root, "")?;
        let attachments = self.root.join(ATTACHMENTS_FOLDER);
        if fs::symlink_metadata(&attachments).is_ok_and(|m| m.is_dir()) {
            files.extend(scan_dir(&attachments, ATTACHMENTS_FOLDER)?);
        }
        files.par_sort_unstable_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }
}

impl FolderLister for FsVault {
    /// Walks folders in parallel, reading only their entries' types (no file is looked at).
    fn folders(&self) -> io::Result<Vec<String>> {
        let mut folders = folders_in(&self.root, "")?;
        folders.par_sort_unstable();
        Ok(folders)
    }
}

/// The visible folders below `abs` (its path `rel` in the vault), each with those below it.
fn folders_in(abs: &Path, rel: &str) -> io::Result<Vec<String>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(abs)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| !n.starts_with('.')) else { continue };
        if entry.file_type()?.is_dir() {
            dirs.push((entry.path(), if rel.is_empty() { name.to_owned() } else { format!("{rel}/{name}") }));
        }
    }
    let nested: Vec<Vec<String>> = dirs.par_iter().map(|(abs, rel)| skip_unlistable(folders_in(abs, rel), rel)).collect::<io::Result<_>>()?;
    let mut folders: Vec<String> = dirs.into_iter().map(|(_, rel)| rel).collect();
    folders.extend(nested.into_iter().flatten());
    Ok(folders)
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
    let nested: Vec<Vec<VaultFile>> = dirs.par_iter().map(|(abs, rel)| skip_unlistable(scan_dir(abs, rel), rel)).collect::<io::Result<_>>()?;
    files.extend(nested.into_iter().flatten());
    Ok(files)
}

/// A folder below the root that cannot be listed counts as empty: one deleted while the vault
/// is walked is not part of it any more, and one not allowed (its permissions, another user's)
/// cannot be read, by the index or by anyone, without failing the whole vault. The root itself
/// still fails, as the vault does then.
fn skip_unlistable<T>(listed: io::Result<Vec<T>>, rel: &str) -> io::Result<Vec<T>> {
    match listed {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            eprintln!("knotes: {rel} cannot be listed, left out: {e}");
            Ok(Vec::new())
        }
        other => other,
    }
}

fn mtime_ns(meta: &Metadata) -> i64 {
    meta.mtime() * 1_000_000_000 + meta.mtime_nsec()
}

impl VaultReader for FsVault {
    /// A file not in UTF-8 reads lossily, its other bytes as U+FFFD: good for the index and
    /// for showing it, never for writing it back (`read_utf8`).
    fn read_text(&self, path: &str) -> io::Result<String> {
        let bytes = fs::read(self.inside(path)?)?;
        Ok(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }
}

impl FsVault {
    /// The text of the file at `path` exactly as it is, or `InvalidData` when it is not UTF-8:
    /// what is read to be written back, so a file in another encoding never loses its bytes.
    pub fn read_utf8(&self, path: &str) -> io::Result<String> {
        String::from_utf8(fs::read(self.inside(path)?)?).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("{path} is not UTF-8")))
    }
}

/// Name of the per-folder rules file (ADR 0010, 0020).
const RULES_FILE: &str = ".knotes.json";
/// Its old name, read while there is no new file and removed by the next write. At the vault
/// root `.knotes` is now Knotes's folder (ADR 0020).
const OLD_RULES_FILE: &str = ".knotes";

/// The text of the file at `path`; `None` when there is none (or a folder stands there).
fn read_if_file(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::IsADirectory) => Ok(None),
        Err(e) => Err(e),
    }
}

impl FolderRulesStore for FsVault {
    fn read_rules(&self, folders: &[&str]) -> io::Result<Vec<(String, String)>> {
        let mut found = Vec::new();
        for folder in folders {
            let dir = self.root.join(folder);
            let text = match read_if_file(&dir.join(RULES_FILE))? {
                Some(text) => Some(text),
                None => read_if_file(&dir.join(OLD_RULES_FILE))?,
            };
            if let Some(text) = text {
                found.push(((*folder).to_owned(), text));
            }
        }
        Ok(found)
    }

    /// Writes a temporary file next to the target and renames it over (`replace_file`).
    fn write_rules(&self, folder: &str, text: &str) -> io::Result<()> {
        let dir = self.root.join(folder);
        replace_file(&dir.join(RULES_FILE), &dir.join(format!("{RULES_FILE}.tmp")), text)?;
        let old = dir.join(OLD_RULES_FILE);
        if old.is_file() {
            fs::remove_file(old)?;
        }
        Ok(())
    }
}

impl FsVault {
    /// The absolute path of a vault-relative path that stays inside the vault.
    pub(crate) fn inside(&self, path: &str) -> io::Result<PathBuf> {
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

impl AttachmentStore for FsVault {
    /// Writes a hidden temporary file in the folder and renames it to the first free name, so
    /// the watcher never sees half a file and no file is replaced.
    fn add_attachment(&self, name: &str, bytes: &[u8]) -> io::Result<String> {
        if name.is_empty() || name.contains('/') || name.starts_with('.') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a file name: {name}")));
        }
        self.make_knotes_folder()?;
        let dir = self.root.join(ATTACHMENTS_FOLDER);
        fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".{name}.knotes-tmp"));
        fs::write(&tmp, bytes)?;
        let (stem, ext) = name.rsplit_once('.').map_or((name, String::new()), |(s, e)| (s, format!(".{e}")));
        for n in 0.. {
            let candidate = if n == 0 { name.to_owned() } else { format!("{stem} {n}{ext}") };
            match rename_exclusive(&tmp, &dir.join(&candidate)) {
                Ok(()) => return Ok(format!("{ATTACHMENTS_FOLDER}/{candidate}")),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(e);
                }
            }
        }
        unreachable!("some number is free")
    }
}

impl FsVault {
    /// Makes room for `.knotes/` at the vault root: an old root rules file there (ADR 0010)
    /// becomes `.knotes.json`, or goes when a newer one exists.
    pub(crate) fn make_knotes_folder(&self) -> io::Result<()> {
        let old = self.root.join(OLD_RULES_FILE);
        if !old.is_file() {
            return Ok(());
        }
        let new = self.root.join(RULES_FILE);
        if new.exists() { fs::remove_file(old) } else { fs::rename(old, new) }
    }
}

impl NoteWriter for FsVault {
    /// Writes a hidden temporary file next to the note (the scanner and the watcher skip it)
    /// and renames it over the note (`replace_file`).
    fn write_note(&self, path: &str, text: &str) -> io::Result<()> {
        let target = self.inside(path)?;
        let dir = target.parent().unwrap_or(&self.root);
        fs::create_dir_all(dir)?;
        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("note");
        replace_file(&target, &dir.join(format!(".{name}.knotes-tmp")), text)
    }
}

/// Writes `text` into `tmp` and renames it over `target`, so readers (and other devices syncing
/// the vault) never see half a file. What the file it replaces holds besides its text stays: its
/// permissions, creation date, extended attributes (Finder's tags among them) and ACL. The text
/// reaches the disk before the rename, so a crash leaves the old file or the new one, never an
/// empty one.
pub(crate) fn replace_file(target: &Path, tmp: &Path, text: &str) -> io::Result<()> {
    let written = (|| {
        let mut file = fs::File::create(tmp)?;
        io::Write::write_all(&mut file, text.as_bytes())?;
        keep_metadata(target, &file);
        sync(&file)
    })();
    written.and_then(|()| fs::rename(tmp, target)).inspect_err(|_| {
        let _ = fs::remove_file(tmp);
    })
}

/// Gives `file` the permissions, creation date, extended attributes and ACL of the file at
/// `target`, if there is one; what cannot be kept is left as the new file has it.
#[cfg(target_os = "macos")]
fn keep_metadata(target: &Path, file: &fs::File) {
    use std::os::fd::AsRawFd;
    use std::os::macos::fs::FileTimesExt;
    const COPYFILE_ACL: u32 = 1 << 0;
    const COPYFILE_XATTR: u32 = 1 << 2;
    unsafe extern "C" {
        fn fcopyfile(from: i32, to: i32, state: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    let Ok(meta) = fs::metadata(target) else { return };
    let _ = file.set_permissions(meta.permissions());
    if let Ok(created) = meta.created() {
        let _ = file.set_times(fs::FileTimes::new().set_created(created));
    }
    // Not COPYFILE_STAT: it would give the new text the old modification time.
    if let Ok(old) = fs::File::open(target) {
        // SAFETY: two open descriptors that outlive the call; a null state is allowed.
        unsafe { fcopyfile(old.as_raw_fd(), file.as_raw_fd(), std::ptr::null_mut(), COPYFILE_ACL | COPYFILE_XATTR) };
    }
}

#[cfg(not(target_os = "macos"))]
fn keep_metadata(target: &Path, file: &fs::File) {
    if let Ok(meta) = fs::metadata(target) {
        let _ = file.set_permissions(meta.permissions());
    }
}

/// The file's data on the disk. `fsync`, not the drive-cache flush `File::sync_all` makes on
/// macOS (F_FULLFSYNC): that costs milliseconds a file, and a rename rewrites the links of many
/// notes on the main thread (SQLite makes the same choice by default).
#[cfg(target_os = "macos")]
fn sync(file: &fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn fsync(fd: i32) -> i32;
    }
    // SAFETY: an open descriptor.
    if unsafe { fsync(file.as_raw_fd()) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
fn sync(file: &fs::File) -> io::Result<()> {
    file.sync_data()
}
