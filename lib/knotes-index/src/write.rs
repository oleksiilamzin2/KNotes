use std::collections::{BTreeSet, HashSet};

use knotes_core::{BoxError, IndexChange, IndexWriter, Link, LinkKind, ParsedNote, VaultFile};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::SqliteIndex;
use crate::codec::{file_kind_code, link_kind_code};
use crate::link_resolver::{Candidates, reresolve};
use crate::name_key::{file_name_key, link_target_key};

/// A link of a note written in this batch, resolved once all files of the batch are known.
struct NewLink<'a> {
    file_id: i64,
    ord: usize,
    source: &'a str,
    link: &'a Link,
    key: Option<String>,
}

/// FTS work of a batch, done after all other rows: any statement that opens a savepoint
/// (the `files` upsert does) makes FTS5 flush its pending terms to a new segment and merge.
/// Interleaved, that is one segment per note: measured 12 s instead of 4 s for a cold 50k build.
#[derive(Default)]
struct FtsWork<'a> {
    deletes: Vec<i64>,
    inserts: Vec<(i64, &'a str)>,
}

impl IndexWriter for SqliteIndex {
    fn apply(&mut self, changes: &[IndexChange<'_>]) -> Result<(), BoxError> {
        // Texts of the batch go to the pack in one write before the commit; offsets start at
        // the length on disk, so an aborted batch leaves only dead bytes.
        let base = self.pack.len()?;
        let mut texts = Vec::new();
        let mut fts = FtsWork::default();
        // Names of files that came or went (their links re-resolve) and notes with new links.
        let mut changed_keys = HashSet::new();
        let mut links = Vec::new();
        let tx = self.conn.transaction()?;
        for change in changes {
            match change {
                IndexChange::Upsert { file, note } => {
                    let range = note.map(|(source, _)| {
                        let off = base + texts.len() as u64;
                        texts.extend_from_slice(source.as_bytes());
                        (off, source.len() as u64)
                    });
                    let key = file_name_key(&file.path);
                    let (id, is_new) = upsert_file(&tx, file, &key, range)?;
                    if is_new {
                        changed_keys.insert(key);
                    }
                    delete_note_rows(&tx, id, &mut fts)?;
                    if let Some((source, parsed)) = note {
                        insert_note_rows(&tx, id, source, parsed, &mut fts)?;
                        links.extend(parsed.links.iter().enumerate().map(|(ord, link)| NewLink {
                            file_id: id,
                            ord,
                            source: &file.path,
                            link,
                            key: link_target_key(&link.target).filter(|_| link.kind != LinkKind::External),
                        }));
                    }
                }
                IndexChange::Remove { path } => {
                    let found: Option<(i64, String)> = tx
                        .prepare_cached("SELECT id, name_key FROM files WHERE path = ?1")?
                        .query_row([path], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()?;
                    if let Some((id, key)) = found {
                        delete_note_rows(&tx, id, &mut fts)?;
                        tx.prepare_cached("DELETE FROM files WHERE id = ?1")?.execute([id])?;
                        changed_keys.insert(key);
                    }
                }
            }
        }
        // New links are resolved before insertion (no UPDATE per link); older links only when
        // a file with their target name came or went.
        insert_links(&tx, &links)?;
        let written: HashSet<i64> = links.iter().map(|l| l.file_id).collect();
        reresolve(&tx, &changed_keys, &written)?;
        // Deletes first (by rowid; `rowid IN (SELECT ...)` would scan the whole FTS index),
        // then inserts with ascending rowids, so FTS5 buffers them until the commit.
        {
            let mut st = tx.prepare_cached("DELETE FROM blocks_fts WHERE rowid = ?1")?;
            for rowid in &fts.deletes {
                st.execute([rowid])?;
            }
        }
        {
            let mut st = tx.prepare_cached("INSERT INTO blocks_fts(rowid, text) VALUES (?1, ?2)")?;
            for (rowid, text) in &fts.inserts {
                st.execute(params![rowid, text])?;
            }
        }
        self.pack.append(&texts)?;
        tx.commit()?;
        self.invalidate_caches();
        Ok(())
    }
}

/// `pack` is the `(offset, length)` of the note text in the content pack.
/// Returns the file id and whether the file is new to the index.
fn upsert_file(tx: &Transaction, file: &VaultFile, key: &str, pack: Option<(u64, u64)>) -> Result<(i64, bool), BoxError> {
    let (off, len) = match pack {
        Some((off, len)) => (Some(i64::try_from(off)?), Some(i64::try_from(len)?)),
        None => (None, None),
    };
    let existing: Option<i64> = tx.prepare_cached("SELECT id FROM files WHERE path = ?1")?.query_row([&file.path], |r| r.get(0)).optional()?;
    let id = tx
        .prepare_cached(
            "INSERT INTO files(path, kind, size, mtime_ns, name_key, pack_off, pack_len) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET kind = excluded.kind, size = excluded.size, mtime_ns = excluded.mtime_ns,
                 pack_off = excluded.pack_off, pack_len = excluded.pack_len
             RETURNING id",
        )?
        .query_row(
            params![file.path, file_kind_code(file.kind), i64::try_from(file.size)?, file.mtime_ns, key, off, len],
            |r| r.get(0),
        )?;
    Ok((id, existing.is_none()))
}

fn insert_links(tx: &Transaction, links: &[NewLink]) -> Result<(), BoxError> {
    let candidates = Candidates::load(tx, links.iter().filter_map(|l| l.key.as_deref()))?;
    let mut st = tx.prepare_cached(
        "INSERT INTO links(file_id, ord, kind, embed, target, subpath, display, start, end, target_key, target_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for n in links {
        let l = n.link;
        // External links never resolve; `[[#Heading]]` (no key) points into the note itself.
        let target_id = match (&n.key, l.kind) {
            (_, LinkKind::External) => None,
            (None, _) => Some(n.file_id),
            (Some(key), kind) => candidates.choose(kind, &l.target, key, n.source),
        };
        st.execute(params![
            n.file_id,
            n.ord as i64,
            link_kind_code(l.kind),
            l.embed,
            l.target,
            l.subpath,
            l.display,
            l.span.start,
            l.span.end,
            n.key,
            target_id
        ])?;
    }
    Ok(())
}

fn delete_note_rows(tx: &Transaction, id: i64, fts: &mut FtsWork) -> Result<(), BoxError> {
    let mut st = tx.prepare_cached("SELECT id FROM blocks WHERE file_id = ?1")?;
    for rowid in st.query_map([id], |r| r.get(0))? {
        fts.deletes.push(rowid?);
    }
    for table in ["blocks", "headings", "links", "tags"] {
        tx.prepare_cached(&format!("DELETE FROM {table} WHERE file_id = ?1"))?.execute([id])?;
    }
    Ok(())
}

fn insert_note_rows<'a>(
    tx: &Transaction,
    id: i64,
    source: &'a str,
    note: &ParsedNote,
    fts: &mut FtsWork<'a>,
) -> Result<(), BoxError> {
    let mut st = tx.prepare_cached("INSERT INTO blocks(file_id, ord, heading_ord, start, end) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for (ord, b) in note.blocks.iter().enumerate() {
        st.execute(params![id, ord as i64, b.heading, b.span.start, b.span.end])?;
        fts.inserts.push((tx.last_insert_rowid(), &source[b.span.start as usize..b.span.end as usize]));
    }

    let mut st = tx.prepare_cached("INSERT INTO headings(file_id, ord, level, text, start, end) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    for (ord, h) in note.headings.iter().enumerate() {
        st.execute(params![id, ord as i64, h.level, h.text, h.span.start, h.span.end])?;
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
