//! Resolves links to file ids (ADR 0009).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use knotes_core::{BoxError, LinkKind};
use rusqlite::types::Value;
use rusqlite::{Transaction, params};

use crate::codec::link_kind_from;
use crate::name_key::{file_name_key, fold};

struct Candidate<I> {
    /// What the choice returns: a file id in SQLite, the vault path in memory.
    id: I,
    /// Folded vault path.
    path: String,
}

/// Files that links with the given name keys can point to.
pub(crate) struct Candidates<I>(HashMap<String, Vec<Candidate<I>>>);

impl Candidates<i64> {
    pub fn load<'k>(tx: &Transaction, keys: impl IntoIterator<Item = &'k str>) -> Result<Self, BoxError> {
        let keys: HashSet<&str> = keys.into_iter().collect();
        let mut map: HashMap<String, Vec<Candidate<i64>>> = HashMap::new();
        if keys.is_empty() {
            return Ok(Self(map));
        }
        let keys: Rc<Vec<Value>> = Rc::new(keys.into_iter().map(|k| Value::Text(k.to_owned())).collect());
        let mut st = tx.prepare_cached("SELECT id, path, name_key FROM files WHERE name_key IN rarray(?1)")?;
        for row in st.query_map([keys], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, path, key) = row?;
            map.entry(key).or_default().push(Candidate { id, path: fold(&path) });
        }
        Ok(Self::sorted(map))
    }
}

impl Candidates<String> {
    /// All `paths` of the vault, by name key; the id is the path itself.
    pub fn from_paths<'p>(paths: impl IntoIterator<Item = &'p str>) -> Self {
        let mut map: HashMap<String, Vec<Candidate<String>>> = HashMap::new();
        for path in paths {
            map.entry(file_name_key(path)).or_default().push(Candidate { id: path.to_owned(), path: fold(path) });
        }
        Self::sorted(map)
    }
}

impl<I> Candidates<I> {
    /// Shortest path first, then path order: the deterministic pick for ambiguous names.
    fn sorted(mut map: HashMap<String, Vec<Candidate<I>>>) -> Self {
        for list in map.values_mut() {
            list.sort_by(|a, b| (a.path.len(), &a.path).cmp(&(b.path.len(), &b.path)));
        }
        Self(map)
    }

    /// 1. Markdown links: relative to the folder of the source note.
    /// 2. Links with a folder (and Markdown links): exact vault path, then a path ending with it.
    /// 3. Bare names: the candidate with the shortest path.
    pub fn choose(&self, kind: LinkKind, target: &str, key: &str, source: &str) -> Option<&I> {
        let candidates = self.0.get(key)?;
        let target = fold(target);
        let exact =
            |path: &str| candidates.iter().find(|c| c.path == path || c.path.strip_suffix(".md") == Some(path)).map(|c| &c.id);
        if kind == LinkKind::Markdown {
            let folder = fold(source.rsplit_once('/').map_or("", |(dir, _)| dir));
            if let Some(relative) = join_relative(&folder, &target)
                && let Some(id) = exact(&relative)
            {
                return Some(id);
            }
        }
        if kind == LinkKind::Markdown || target.contains('/') {
            let target = target.trim_start_matches('/');
            return exact(target).or_else(|| {
                let suffix = format!("/{target}");
                candidates
                    .iter()
                    .find(|c| c.path.ends_with(&suffix) || c.path.strip_suffix(".md").is_some_and(|p| p.ends_with(&suffix)))
                    .map(|c| &c.id)
            });
        }
        candidates.first().map(|c| &c.id)
    }
}

/// Re-resolves stored links whose target key is in `changed_keys` (a file with that name came
/// or went). Links of the notes in `skip` were just resolved against the final file set.
pub(crate) fn reresolve(tx: &Transaction, changed_keys: &HashSet<String>, skip: &HashSet<i64>) -> Result<(), BoxError> {
    if changed_keys.is_empty() {
        return Ok(());
    }
    let keys: Rc<Vec<Value>> = Rc::new(changed_keys.iter().map(|k| Value::Text(k.clone())).collect());
    let mut st = tx.prepare_cached(
        "SELECT l.file_id, l.ord, l.kind, l.target, l.target_key, l.target_id, s.path
         FROM links l JOIN files s ON s.id = l.file_id WHERE l.target_key IN rarray(?1)",
    )?;
    let rows = st.query_map([keys], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            link_kind_from(r.get(2)?),
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let links: Vec<_> = rows.filter(|r| r.as_ref().map_or(true, |l| !skip.contains(&l.0))).collect::<Result<_, _>>()?;
    let candidates = Candidates::load(tx, changed_keys.iter().map(String::as_str))?;
    let mut update = tx.prepare_cached("UPDATE links SET target_id = ?3 WHERE file_id = ?1 AND ord = ?2")?;
    for (file_id, ord, kind, target, key, current, source) in links {
        let found = candidates.choose(kind, &target, &key, &source).copied();
        if found != current {
            update.execute(params![file_id, ord, found])?;
        }
    }
    Ok(())
}

/// `folder` + `target` with `.` and `..` segments applied; `None` if `..` leaves the vault.
fn join_relative(folder: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}
