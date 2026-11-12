//! Append-only file with the text of every indexed note (ADR 0008).

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

pub(crate) const PACK_FILE: &str = "content.pack";
/// Magic and generation, before the first text. The generation is also stored in SQLite; a
/// mismatch (e.g. a crash during compaction) means the index must be rebuilt.
pub(crate) const HEADER_LEN: u64 = 16;
const MAGIC: &[u8; 8] = b"KNPACK01";

/// Bytes are only appended, never rewritten or truncated while the pack is open, so a
/// mapping stays valid for its whole length. A newer note version is appended again and
/// the old range becomes dead (compaction will rewrite the file under a new name).
pub(crate) struct ContentPack {
    file: File,
    generation: u64,
    map: RefCell<Option<Arc<Mmap>>>,
}

impl ContentPack {
    /// Opens the pack, creating it (generation 0) if missing. A file without a valid header
    /// is an `InvalidData` error.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).read(true).append(true).open(path)?;
        if file.metadata()?.len() == 0 {
            (&file).write_all(&header(0))?;
        }
        let mut head = [0u8; HEADER_LEN as usize];
        // Positional read: in append mode the cursor sits at the end after writing the header.
        file.read_exact_at(&mut head, 0).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "content pack has no header"))?;
        if &head[..8] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a content pack"));
        }
        let generation = u64::from_le_bytes(head[8..].try_into().expect("8 bytes"));
        Ok(Self { file, generation, map: RefCell::new(None) })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Writes a new pack of generation `generation` with the given ranges of this one, synced
    /// to disk. Returns the new offset of each range, in order.
    pub fn write_compacted(&self, to: &Path, generation: u64, ranges: &[(u64, u64)]) -> io::Result<Vec<u64>> {
        let end = ranges.iter().map(|(off, len)| off + len).max().unwrap_or(0);
        let map = self.map_covering(end)?;
        let mut out = BufWriter::new(File::create(to)?);
        out.write_all(&header(generation))?;
        let mut pos = HEADER_LEN;
        let mut offsets = Vec::with_capacity(ranges.len());
        for &(off, len) in ranges {
            if let Some(map) = &map {
                out.write_all(&map[off as usize..(off + len) as usize])?;
            }
            offsets.push(pos);
            pos += len;
        }
        out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        Ok(offsets)
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
        let map = self.map_covering(end)?.ok_or_else(|| io::Error::other("empty content pack"))?;
        String::from_utf8(map[offset as usize..end as usize].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// A mapping that covers `end` (`None` for `end == 0`), remapped when the pack has grown
    /// since the last map. Older mappings handed out stay valid.
    pub fn map_covering(&self, end: u64) -> io::Result<Option<Arc<Mmap>>> {
        if end == 0 {
            return Ok(None);
        }
        let mut slot = self.map.borrow_mut();
        if let Some(map) = slot.as_ref().filter(|m| m.len() as u64 >= end) {
            return Ok(Some(Arc::clone(map)));
        }
        // SAFETY: the pack is append-only and never truncated while open (see type docs),
        // so the mapped bytes cannot change or disappear under the mapping.
        let map = Arc::new(unsafe { Mmap::map(&self.file)? });
        if (map.len() as u64) < end {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "content pack is shorter than the index says"));
        }
        *slot = Some(Arc::clone(&map));
        Ok(Some(map))
    }
}

fn header(generation: u64) -> [u8; HEADER_LEN as usize] {
    let mut h = [0u8; HEADER_LEN as usize];
    h[..8].copy_from_slice(MAGIC);
    h[8..].copy_from_slice(&generation.to_le_bytes());
    h
}
