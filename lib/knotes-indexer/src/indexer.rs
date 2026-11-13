use std::io;

use knotes_core::{
    Block, BoxError, DocumentText, FileKind, FileStates, IndexChange, IndexWriter, NoDocuments, NoteParser, ParsedNote, Span, VaultFile, VaultReader, VaultScanner,
};
use rayon::prelude::*;

use crate::plan::plan;

/// Files registered per transaction in the first pass (metadata only).
const REGISTER_BATCH: usize = 10_000;
/// Notes read, parsed and written per transaction in the second pass.
const CONTENT_BATCH: usize = 2_000;

#[derive(Debug, Default)]
pub struct SyncReport {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    /// Vault path and error of notes that could not be read; they are retried next sync.
    pub failed: Vec<(String, String)>,
}

/// Most bytes of a PDF's text in one searched block.
const DOCUMENT_BLOCK: usize = 2048;
/// Characters of a PDF's text shown in the note list.
const DOCUMENT_SNIPPET: usize = 200;

/// Brings an index up to date with the vault: scan, compare with the index, write changes.
/// Notes are parsed; PDFs' text (ADR 0022) comes from `documents`.
pub struct Indexer<V, P, D = NoDocuments> {
    vault: V,
    parser: P,
    documents: D,
}

impl<V: VaultScanner + VaultReader + Sync, P: NoteParser + Sync> Indexer<V, P, NoDocuments> {
    pub fn new(vault: V, parser: P) -> Self {
        Self { vault, parser, documents: NoDocuments }
    }
}

impl<V: VaultScanner + VaultReader + Sync, P: NoteParser + Sync, D: DocumentText + Sync> Indexer<V, P, D> {
    pub fn with_documents(vault: V, parser: P, documents: D) -> Self {
        Self { vault, parser, documents }
    }

    pub fn sync(&self, index: &mut (impl IndexWriter + FileStates)) -> Result<SyncReport, BoxError> {
        let scanned = self.vault.scan()?;
        let indexed = index.file_states()?;
        let plan = plan(&scanned, &indexed);
        let mut report =
            SyncReport { added: plan.added.len(), modified: plan.modified.len(), removed: plan.removed.len(), failed: Vec::new() };

        // Removals first, so deleted files stop being link targets before new links resolve.
        for chunk in plan.removed.chunks(CONTENT_BATCH) {
            index.apply(&chunk.iter().map(|path| IndexChange::Remove { path }).collect::<Vec<_>>())?;
        }

        // Pass 1: register new files without content and with mtime 0 ("not indexed yet").
        // Every link target is then known before any note's links resolve, and a note that
        // fails to read below keeps mtime 0 and is retried by the next sync.
        let placeholders: Vec<VaultFile> = plan.added.iter().map(|f| VaultFile { mtime_ns: 0, ..(*f).clone() }).collect();
        for chunk in placeholders.chunks(REGISTER_BATCH) {
            index.apply(&chunk.iter().map(|file| IndexChange::Upsert { file, note: None }).collect::<Vec<_>>())?;
        }

        // Pass 2: read and parse in parallel, write each batch in one transaction.
        let todo: Vec<&VaultFile> = plan.added.iter().chain(&plan.modified).copied().collect();
        for chunk in todo.chunks(CONTENT_BATCH) {
            let loaded: Vec<io::Result<Option<(String, ParsedNote)>>> = chunk.par_iter().map(|f| self.load(f)).collect();
            let mut changes = Vec::with_capacity(chunk.len());
            for (file, result) in chunk.iter().zip(&loaded) {
                match result {
                    Ok(note) => changes.push(IndexChange::Upsert { file, note: note.as_ref().map(|(t, p)| (t.as_str(), p)) }),
                    // Deleted between the scan and the read.
                    Err(e) if e.kind() == io::ErrorKind::NotFound => changes.push(IndexChange::Remove { path: &file.path }),
                    Err(e) => report.failed.push((file.path.clone(), e.to_string())),
                }
            }
            index.apply(&changes)?;
        }
        Ok(report)
    }

    /// Text and parse result for notes, text in plain blocks for PDFs; `None` for other files.
    fn load(&self, file: &VaultFile) -> io::Result<Option<(String, ParsedNote)>> {
        if file.kind == FileKind::Pdf {
            return Ok(self.documents.document_text(&file.path).map(|text| {
                let parsed = plain_text(&text);
                (text, parsed)
            }));
        }
        if file.kind != FileKind::Markdown {
            return Ok(None);
        }
        let text = self.vault.read_text(&file.path)?;
        let parsed = self.parser.parse(&text);
        Ok(Some((text, parsed)))
    }
}

/// A document's text as it is: searched in blocks of paragraphs (no Markdown read into it: a
/// `#word` in a PDF is no tag), its start for the note list.
fn plain_text(text: &str) -> ParsedNote {
    let mut blocks = Vec::new();
    let mut start = 0;
    for (at, _) in text.match_indices("\n\n").chain(std::iter::once((text.len(), ""))) {
        // A block ends at a paragraph break once it is long enough, or where it must.
        let mut end = at;
        while end - start > DOCUMENT_BLOCK {
            let cut = text.floor_char_boundary(start + DOCUMENT_BLOCK);
            blocks.push(Block { span: Span::new(start, cut), heading: None });
            start = cut;
            end = at;
        }
        if end > start && (end - start >= DOCUMENT_BLOCK / 2 || at == text.len()) {
            blocks.push(Block { span: Span::new(start, end), heading: None });
            start = end;
        }
    }
    let snippet: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(DOCUMENT_SNIPPET).collect();
    ParsedNote { frontmatter: None, headings: Vec::new(), links: Vec::new(), tags: Vec::new(), anchors: Vec::new(), blocks, snippet }
}
