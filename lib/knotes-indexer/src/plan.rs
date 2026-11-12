use std::cmp::Ordering;

use knotes_core::{FileState, VaultFile};

/// What a sync has to do: files new to the index, files whose size or mtime changed, and
/// indexed paths that are gone from the vault.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan<'a> {
    pub added: Vec<&'a VaultFile>,
    pub modified: Vec<&'a VaultFile>,
    pub removed: Vec<&'a str>,
}

/// Merge of two path-sorted lists. An indexed mtime of 0 marks a file registered but not yet
/// indexed, so it always counts as modified.
pub fn plan<'a>(scanned: &'a [VaultFile], indexed: &'a [FileState]) -> Plan<'a> {
    let mut plan = Plan::default();
    let (mut s, mut i) = (scanned.iter().peekable(), indexed.iter().peekable());
    loop {
        match (s.peek(), i.peek()) {
            (Some(f), Some(st)) => match f.path.as_str().cmp(st.path.as_str()) {
                Ordering::Less => plan.added.push(s.next().unwrap()),
                Ordering::Greater => plan.removed.push(&i.next().unwrap().path),
                Ordering::Equal => {
                    if f.size != st.size || f.mtime_ns != st.mtime_ns {
                        plan.modified.push(f);
                    }
                    s.next();
                    i.next();
                }
            },
            (Some(_), None) => plan.added.push(s.next().unwrap()),
            (None, Some(_)) => plan.removed.push(&i.next().unwrap().path),
            (None, None) => return plan,
        }
    }
}

#[cfg(test)]
mod tests {
    use knotes_core::FileKind;

    use super::*;

    fn file(path: &str, size: u64, mtime_ns: i64) -> VaultFile {
        VaultFile { path: path.into(), kind: FileKind::Markdown, size, mtime_ns }
    }

    fn state(path: &str, size: u64, mtime_ns: i64) -> FileState {
        FileState { path: path.into(), size, mtime_ns }
    }

    #[test]
    fn finds_added_modified_and_removed() {
        let scanned = [file("a", 1, 1), file("b", 2, 2), file("d", 1, 1), file("e", 5, 5)];
        let indexed = [state("a", 1, 1), state("b", 2, 3), state("c", 1, 1), state("e", 5, 5)];
        let p = plan(&scanned, &indexed);
        assert_eq!(p.added.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["d"]);
        assert_eq!(p.modified.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["b"]);
        assert_eq!(p.removed, ["c"]);
    }

    #[test]
    fn registered_but_unindexed_files_count_as_modified() {
        let (scanned, indexed) = ([file("a", 1, 7)], [state("a", 1, 0)]);
        let p = plan(&scanned, &indexed);
        assert_eq!(p.modified.len(), 1);
    }
}
