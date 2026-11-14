//! The connections of a document gathered off the main thread (ADR 0026): the index asked
//! through a reader of its own, then each note read and parsed for the lines.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use knotes_app::{Mention, backlink_mentions, unlinked_mentions};
use knotes_core::{ContentQuery, ContentSearch, LinkGraph, NoteParser, VaultReader, Within};
use knotes_index::{LinkTargets, SqliteIndex};
use knotes_md::MarkdownParser;
use knotes_vault::FsVault;

/// Notes with their mentions of a document.
pub type Mentions = Vec<(String, Vec<Mention>)>;

/// The notes linking to `document` and the notes naming it (`name`) without a link, with the
/// lines; `None` when the index does not open or `cancelled` is set meanwhile.
pub fn gather(vault: &Path, index_dir: &Path, document: &str, name: &str, links: &LinkTargets, cancelled: &AtomicBool) -> Option<(Mentions, Mentions)> {
    let reader = SqliteIndex::open_reader(index_dir).ok()?;
    let mut sources: Vec<String> = reader.backlinks(document).ok()?.into_iter().map(|b| b.source).collect();
    sources.dedup();
    let query = ContentQuery { text: name, within: Within::Note, prefixes: false, scope: None, limit: 200 };
    let mut named: Vec<String> = reader.search(&query).unwrap_or_default().into_iter().map(|h| h.path).filter(|p| p != document).collect();
    named.sort();
    named.dedup();
    let files = FsVault::new(vault);
    let parser = MarkdownParser::new();
    let mut linked = Vec::new();
    for source in &sources {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(text) = files.read_text(source) else { continue };
        let mentions = backlink_mentions(document, source, &text, &parser.parse(&text), links);
        if !mentions.is_empty() {
            linked.push((source.clone(), mentions));
        }
    }
    let mut unlinked = Vec::new();
    for source in &named {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(text) = files.read_text(source) else { continue };
        let mentions = unlinked_mentions(name, &text, &parser.parse(&text));
        if !mentions.is_empty() {
            unlinked.push((source.clone(), mentions));
        }
    }
    Some((linked, unlinked))
}
