use crate::LinkKind;

/// Resolves links without the index, for text that is being edited (ADR 0009, 0013).
pub trait LinkResolver {
    /// Vault path of the file a link written in the note `source` points to; `None` when it is
    /// missing or external. A link without a target (`[[#Heading]]`) points to `source`.
    fn resolve(&self, kind: LinkKind, target: &str, source: &str) -> Option<String>;

    /// Shortest wikilink target that resolves to `path` from `source`: the file name (without
    /// `.md` for notes) when it does, otherwise the full vault path.
    fn link_text(&self, path: &str, source: &str) -> String;
}
