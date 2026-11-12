use knotes_core::{BackLink, BoxError, FileState, FileStates, Heading, Link, LinkGraph, NoteOutline, NoteText, OutLink, Span, TagIndex};
use rusqlite::OptionalExtension;

use crate::SqliteIndex;
use crate::codec::link_kind_from;

impl FileStates for SqliteIndex {
    fn file_states(&self) -> Result<Vec<FileState>, BoxError> {
        let mut st = self.conn.prepare_cached("SELECT path, size, mtime_ns FROM files ORDER BY path")?;
        let rows = st.query_map([], |r| Ok(FileState { path: r.get(0)?, size: r.get::<_, i64>(1)? as u64, mtime_ns: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl NoteOutline for SqliteIndex {
    fn headings(&self, path: &str) -> Result<Vec<Heading>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT h.level, h.text, h.start, h.end FROM headings h JOIN files f ON f.id = h.file_id
             WHERE f.path = ?1 ORDER BY h.ord",
        )?;
        let rows = st.query_map([path], |r| {
            Ok(Heading { level: r.get(0)?, text: r.get(1)?, span: Span { start: r.get(2)?, end: r.get(3)? } })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// Columns 0..=6 of a `links l` row.
const LINK_COLUMNS: &str = "l.kind, l.embed, l.target, l.subpath, l.display, l.start, l.end";

fn link_at(r: &rusqlite::Row) -> rusqlite::Result<Link> {
    Ok(Link {
        kind: link_kind_from(r.get(0)?),
        embed: r.get(1)?,
        target: r.get(2)?,
        subpath: r.get(3)?,
        display: r.get(4)?,
        span: Span { start: r.get(5)?, end: r.get(6)? },
    })
}

impl LinkGraph for SqliteIndex {
    fn links_from(&self, path: &str) -> Result<Vec<OutLink>, BoxError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {LINK_COLUMNS}, t.path FROM links l JOIN files f ON f.id = l.file_id
             LEFT JOIN files t ON t.id = l.target_id WHERE f.path = ?1 ORDER BY l.ord"
        ))?;
        let rows = st.query_map([path], |r| Ok(OutLink { link: link_at(r)?, resolved: r.get(7)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn backlinks(&self, path: &str) -> Result<Vec<BackLink>, BoxError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {LINK_COLUMNS}, s.path FROM files t JOIN links l ON l.target_id = t.id JOIN files s ON s.id = l.file_id
             WHERE t.path = ?1 AND l.file_id != t.id ORDER BY s.path, l.ord"
        ))?;
        let rows = st.query_map([path], |r| Ok(BackLink { link: link_at(r)?, source: r.get(7)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl TagIndex for SqliteIndex {
    fn tags_of(&self, path: &str) -> Result<Vec<String>, BoxError> {
        let mut st =
            self.conn.prepare_cached("SELECT t.tag FROM tags t JOIN files f ON f.id = t.file_id WHERE f.path = ?1 ORDER BY t.tag")?;
        let rows = st.query_map([path], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn notes_with_tag(&self, tag: &str) -> Result<Vec<String>, BoxError> {
        // `tag/...` children form the range [tag + '/', tag + '0'): '0' is the byte after '/'.
        let mut st = self.conn.prepare_cached(
            "SELECT DISTINCT f.path FROM tags t JOIN files f ON f.id = t.file_id
             WHERE t.tag = ?1 OR (t.tag >= ?1 || '/' AND t.tag < ?1 || '0') ORDER BY f.path",
        )?;
        let rows = st.query_map([tag.to_lowercase()], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl NoteText for SqliteIndex {
    fn note_text(&self, path: &str) -> Result<Option<String>, BoxError> {
        let range: Option<(Option<i64>, Option<i64>)> = self
            .conn
            .prepare_cached("SELECT pack_off, pack_len FROM files WHERE path = ?1")?
            .query_row([path], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        match range {
            Some((Some(off), Some(len))) => Ok(Some(self.pack.read_string(off as u64, len as u64)?)),
            _ => Ok(None),
        }
    }
}
