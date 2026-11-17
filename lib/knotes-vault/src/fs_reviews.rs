//! The vault's review logs (ADR 0037), in `.knotes/reviews`: a file a Mac (`<name>.tsv`), a line
//! a review, only ever appended to, so that two Macs syncing the vault never write one file.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::FsVault;

/// Where the logs are, inside the vault.
const REVIEWS: &str = ".knotes/reviews";
const EXTENSION: &str = "tsv";

impl FsVault {
    /// Appends `line` (a review's, with its line break) to the log `name`, made with its folder
    /// when it is the first.
    pub fn append_review(&self, name: &str, line: &str) -> io::Result<()> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a review log's name: {name}")));
        }
        let dir = self.root.join(REVIEWS);
        fs::create_dir_all(&dir)?;
        let mut file = fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{name}.{EXTENSION}")))?;
        file.write_all(line.as_bytes())?;
        file.sync_data()
    }

    /// The logs there are, each with its size in bytes, sorted by name; none without the folder.
    pub fn review_logs(&self) -> io::Result<Vec<(String, u64)>> {
        let Ok(entries) = fs::read_dir(self.root.join(REVIEWS)) else { return Ok(Vec::new()) };
        let mut logs: Vec<(String, u64)> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == EXTENSION))
            .filter_map(|e| Some((e.path().file_stem()?.to_str()?.to_owned(), e.metadata().ok()?.len())))
            .collect();
        logs.sort();
        Ok(logs)
    }

    /// What the log `name` holds from byte `from` on.
    pub fn read_review_log(&self, name: &str, from: u64) -> io::Result<String> {
        let mut file = fs::File::open(self.root.join(REVIEWS).join(format!("{name}.{EXTENSION}")))?;
        file.seek(SeekFrom::Start(from))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
