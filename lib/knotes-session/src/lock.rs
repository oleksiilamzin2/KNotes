use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;

/// Exclusive lock on a vault's index folder, held while a window has the vault open; keeps a
/// second window or app instance from writing the same index.
pub struct VaultLock {
    _file: File,
}

impl VaultLock {
    pub fn acquire(index_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(index_dir)?;
        let file = File::options().create(true).truncate(false).write(true).open(index_dir.join("lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(io::Error::new(io::ErrorKind::ResourceBusy, "this vault is already open")),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }
}
