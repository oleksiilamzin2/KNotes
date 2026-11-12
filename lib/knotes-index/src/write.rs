use std::collections::BTreeSet;

use knotes_core::{BoxError, IndexChange, IndexWriter, ParsedNote, VaultFile};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::SqliteIndex;
use crate::codec::{file_kind_code, link_kind_code};

impl IndexWriter for SqliteIndex {
    fn apply(&mut self, changes: &[IndexChange<'_>]) -> Result<(), BoxError> {
        // Texts of the batch go to the pack in one write before the commit; offsets start at
        // the length on disk, so an aborted batch leaves only dead bytes.
        let base = self.pack.len()?;
        let mut texts = Vec::new();
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                IndexChange::Upsert { file, note } => {
                    let range = note.map(|(source, _)| {
                        let off = base + texts.len() as u64;
                        texts.extend_from_slice(source.as_bytes());
                        (off, source.len() as u64)
                    });
                    let id = upsert_file(&tx, file, range)?;
                    delete_note_rows(&tx, id)?;
                    if let Some((_source, parsed)) = note {
                        insert_note_rows(&tx, id, parsed)?;
                    }
                }
                IndexChange::Remove { path } => {
                    let id: Option<i64> =
                        tx.prepare_cached("SELECT id FROM files WHERE path = ?1")?.query_row([path], |r| r.get(0)).optional()?;
                    if let Some(id) = id {
                        delete_note_rows(&tx, id)?;
                        tx.prepare_cached("DELETE FROM files WHERE id = ?1")?.execute([id])?;
                    }
                }
            }
        }
        self.pack.append(&texts)?;
        tx.commit()?;
        Ok(())
    }
}

/// `pack` is the `(offset, length)` of the note text in the content pack.
fn upsert_file(tx: &Transaction, file: &VaultFile, pack: Option<(u64, u64)>) -> Result<i64, BoxError> {
    let (off, len) = match pack {
        Some((off, len)) => (Some(i64::try_from(off)?), Some(i64::try_from(len)?)),
        None => (None, None),
    };
    let id = tx
        .prepare_cached(
            "INSERT INTO files(path, kind, size, mtime_ns, pack_off, pack_len) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET kind = excluded.kind, size = excluded.size, mtime_ns = excluded.mtime_ns,
                 pack_off = excluded.pack_off, pack_len = excluded.pack_len
             RETURNING id",
        )?
        .query_row(params![file.path, file_kind_code(file.kind), i64::try_from(file.size)?, file.mtime_ns, off, len], |r| {
            r.get(0)
        })?;
    Ok(id)
}

fn delete_note_rows(tx: &Transaction, id: i64) -> Result<(), BoxError> {
    for table in ["headings", "links", "tags"] {
        tx.prepare_cached(&format!("DELETE FROM {table} WHERE file_id = ?1"))?.execute([id])?;
    }
    Ok(())
}

fn insert_note_rows(tx: &Transaction, id: i64, note: &ParsedNote) -> Result<(), BoxError> {
    let mut st = tx.prepare_cached("INSERT INTO headings(file_id, ord, level, text, start, end) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    for (ord, h) in note.headings.iter().enumerate() {
        st.execute(params![id, ord as i64, h.level, h.text, h.span.start, h.span.end])?;
    }

    let mut st = tx.prepare_cached(
        "INSERT INTO links(file_id, ord, kind, embed, target, subpath, display, start, end)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (ord, l) in note.links.iter().enumerate() {
        st.execute(params![id, ord as i64, link_kind_code(l.kind), l.embed, l.target, l.subpath, l.display, l.span.start, l.span.end])?;
    }

    let inline = note.tags.iter().map(|t| t.name.as_str());
    let front = note.frontmatter.iter().flat_map(|f| f.tags.iter().map(String::as_str));
    let tags: BTreeSet<String> = inline.chain(front).map(str::to_lowercase).collect();
    let mut st = tx.prepare_cached("INSERT INTO tags(file_id, tag) VALUES (?1, ?2)")?;
    for tag in &tags {
        st.execute(params![id, tag])?;
    }
    Ok(())
}
