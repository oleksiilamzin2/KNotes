//! Link resolution in memory, with the index's rules (ADR 0009, 0013).

use knotes_core::{LinkKind, LinkResolver};

use crate::link_resolver::Candidates;
use crate::name_key::{link_target_key, strip_md};

/// Every vault path by folded file name; built once per file set, then read-only.
pub struct LinkTargets(Candidates<String>);

impl LinkTargets {
    pub fn new<'p>(paths: impl IntoIterator<Item = &'p str>) -> Self {
        Self(Candidates::from_paths(paths))
    }
}

impl LinkResolver for LinkTargets {
    fn resolve(&self, kind: LinkKind, target: &str, source: &str) -> Option<String> {
        if kind == LinkKind::External {
            return None;
        }
        match link_target_key(target) {
            None => Some(source.to_owned()),
            Some(key) => self.0.choose(kind, target, &key, source).cloned(),
        }
    }

    fn link_text(&self, path: &str, source: &str) -> String {
        let name = strip_md(path.rsplit('/').next().unwrap_or(path));
        if self.resolve(LinkKind::Wiki, name, source).as_deref() == Some(path) {
            name.to_owned()
        } else {
            strip_md(path).to_owned()
        }
    }
}
