use knotes_core::{BackLink, BoxError, CatalogEntry, FileKind, FileState, FileStates, NoteRow, NoteRows, Heading, Link, LinkGraph, NoteCatalog, NoteOutline, NoteText, OutLink, Span, TagIndex};
use std::collections::{BTreeMap, HashSet};

use rusqlite::OptionalExtension;

use crate::SqliteIndex;
use crate::codec::{file_kind_code, link_kind_from};

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

    fn tag_counts(&self) -> Result<Vec<(String, usize)>, BoxError> {
        // Each (tag, note) row counts for the tag and all its parents, once per note.
        let mut notes: BTreeMap<String, HashSet<i64>> = BTreeMap::new();
        let mut st = self.conn.prepare_cached("SELECT tag, file_id FROM tags")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (tag, file) = row?;
            for (i, _) in tag.match_indices('/') {
                notes.entry(tag[..i].to_owned()).or_default().insert(file);
            }
            notes.entry(tag).or_default().insert(file);
        }
        Ok(notes.into_iter().map(|(tag, files)| (tag, files.len())).collect())
    }

    fn untagged_notes(&self) -> Result<Vec<String>, BoxError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT path FROM files f WHERE kind = {} AND NOT EXISTS (SELECT 1 FROM tags t WHERE t.file_id = f.id) ORDER BY path",
            file_kind_code(FileKind::Markdown)
        ))?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl NoteText for SqliteIndex {
    fn note_text(&self, path: &str) -> Result<Option<String>, BoxError> {
        let range: Option<(Option<i64>, Option<i64>, i64)> = self
            .conn
            .prepare_cached(
                "SELECT pack_off, pack_len, (SELECT value FROM meta WHERE key = 'pack_generation') FROM files WHERE path = ?1",
            )?
            .query_row([path], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        match range {
            Some((Some(off), Some(len), generation)) => {
                self.ensure_pack_generation(generation)?;
                Ok(Some(self.pack.borrow().read_string(off as u64, len as u64)?))
            }
            _ => Ok(None),
        }
    }
}

impl NoteCatalog for SqliteIndex {
    fn catalog(&self) -> Result<Vec<CatalogEntry>, BoxError> {
        let mut st = self.conn.prepare_cached(
            "SELECT f.path, a.alias FROM files f LEFT JOIN aliases a ON a.file_id = f.id ORDER BY f.path, a.ord",
        )?;
        let mut entries: Vec<CatalogEntry> = Vec::new();
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))? {
            let (path, alias) = row?;
            if entries.last().is_none_or(|e| e.path != path) {
                entries.push(CatalogEntry { path, aliases: Vec::new() });
            }
            if let (Some(alias), Some(entry)) = (alias, entries.last_mut()) {
                entry.aliases.push(alias);
            }
        }
        Ok(entries)
    }
}

impl NoteRows for SqliteIndex {
    fn note_rows(&self) -> Result<Vec<NoteRow>, BoxError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT path, mtime_ns, COALESCE(snippet, '') FROM files WHERE kind IN ({}, {}) ORDER BY path",
            file_kind_code(FileKind::Markdown),
            file_kind_code(FileKind::Pdf)
        ))?;
        let rows = st.query_map([], |r| Ok(NoteRow { path: r.get(0)?, mtime_ns: r.get(1)?, snippet: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
