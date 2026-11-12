//! Append-only file with the text of every indexed note (ADR 0008).

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

pub(crate) const PACK_FILE: &str = "content.pack";

/// Bytes are only appended, never rewritten or truncated while the pack is open, so a
/// mapping stays valid for its whole length. A newer note version is appended again and
/// the old range becomes dead (compaction will rewrite the file under a new name).
pub(crate) struct ContentPack {
    file: File,
    map: RefCell<Option<Arc<Mmap>>>,
}

impl ContentPack {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).read(true).append(true).open(path)?;
        Ok(Self { file, map: RefCell::new(None) })
    }

    /// Length on disk; offsets of a new batch start here, so a failed partial write
    /// only leaves dead bytes behind.
    pub fn len(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    pub fn append(&self, bytes: &[u8]) -> io::Result<()> {
        (&self.file).write_all(bytes)
    }

    pub fn read_string(&self, offset: u64, len: u64) -> io::Result<String> {
        if len == 0 {
            return Ok(String::new());
        }
        let end = offset + len;
        let map = self.mapped_to(end)?;
        String::from_utf8(map[offset as usize..end as usize].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// A mapping that covers `end`, remapped when the pack has grown since the last map.
    fn mapped_to(&self, end: u64) -> io::Result<Arc<Mmap>> {
        let mut slot = self.map.borrow_mut();
        if let Some(map) = slot.as_ref().filter(|m| m.len() as u64 >= end) {
            return Ok(Arc::clone(map));
        }
        // SAFETY: the pack is append-only and never truncated while open (see type docs),
        // so the mapped bytes cannot change or disappear under the mapping.
        let map = Arc::new(unsafe { Mmap::map(&self.file)? });
        if (map.len() as u64) < end {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "content pack is shorter than the index says"));
        }
        *slot = Some(Arc::clone(&map));
        Ok(map)
    }
}
