use std::collections::BTreeMap;

use knotes_core::{ContentQuery, DocumentPages, NoteParser, TitleSearch, VaultReader, Within};
use knotes_md::MarkdownParser;
use knotes_search::TitleIndex;
use serde_json::{Value, json};

use crate::{ToolError, ToolIndex, ToolSpec, specs};

/// A fragment's text past this many characters is cut.
const FRAGMENT: usize = 400;
/// A read stops past this many characters, at a block's or page's end.
const READ_BUDGET: usize = 24_000;

/// The vault's read-only tools (ADR 0027) over its index, its files and its PDFs' pages.
pub struct VaultTools<I, F, D> {
    index: I,
    files: F,
    documents: D,
    titles: TitleIndex,
}

impl<I: ToolIndex, F: VaultReader, D: DocumentPages> VaultTools<I, F, D> {
    pub fn new(index: I, files: F, documents: D) -> Self {
        let titles = TitleIndex::new(index.catalog().unwrap_or_default().into_iter().map(|e| (e.path, e.aliases)));
        Self { index, files, documents, titles }
    }

    /// The tools' descriptions.
    pub fn specs() -> Vec<ToolSpec> {
        specs()
    }

    /// Runs tool `name` with `args` (a JSON object).
    pub fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        match name {
            "search" => self.search(args),
            "find_by_title" => self.find_by_title(args),
            "read_note" => self.read_note(args),
            "read_pdf" => self.read_pdf(args),
            "outline" => self.outline(args),
            "links" => self.links(args),
            "list_notes" => self.list_notes(args),
            "list_tags" => self.list_tags(),
            _ => Err(ToolError::Unknown(name.to_owned())),
        }
    }

    fn search(&self, args: &Value) -> Result<Value, ToolError> {
        let query = text_arg(args, "query")?;
        let limit = count_arg(args, "limit", 10, 30);
        let hits = self.index.search(&ContentQuery { text: &query, within: Within::Block, prefixes: true, scope: None, limit }).map_err(failed)?;
        let results: Vec<Value> = hits
            .into_iter()
            .map(|hit| {
                let text = self.index.note_text(&hit.path).ok().flatten().and_then(|t| t.get(hit.span.start as usize..hit.span.end as usize).map(cut)).unwrap_or_default();
                json!({"path": hit.path, "title": title_of(&hit.path), "kind": kind_of(&hit.path), "heading": hit.heading, "block": hit.ord, "text": text})
            })
            .collect();
        Ok(json!({"results": results}))
    }

    fn find_by_title(&self, args: &Value) -> Result<Value, ToolError> {
        let query = text_arg(args, "query")?;
        let limit = count_arg(args, "limit", 10, 30);
        let results: Vec<Value> = self.titles.search_titles(&query, limit).into_iter().map(|h| json!({"path": h.path, "title": title_of(&h.path), "kind": kind_of(&h.path)})).collect();
        Ok(json!({"results": results}))
    }

    fn read_note(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        if kind_of(&path) == "pdf" {
            return Err(ToolError::BadArguments(format!("{path} is a PDF: read it with read_pdf")));
        }
        let text = self.files.read_text(&path).map_err(|_| ToolError::NotFound(path.clone()))?;
        let parsed = MarkdownParser::new().parse(&text);
        let from = count_arg(args, "from_block", 0, usize::MAX);
        let most = count_arg(args, "max_blocks", 20, 60).max(1);
        let mut used = 0;
        let mut blocks = Vec::new();
        for (i, block) in parsed.blocks.iter().enumerate().skip(from).take(most) {
            let body = text.get(block.span.start as usize..block.span.end as usize).unwrap_or_default();
            if used > 0 && used + body.len() > READ_BUDGET {
                break;
            }
            used += body.len();
            let heading = block.heading.and_then(|h| parsed.headings.get(h as usize)).map(|h| h.text.clone());
            blocks.push(json!({"block": i, "heading": heading, "text": body}));
        }
        Ok(json!({"path": path, "title": title_of(&path), "total_blocks": parsed.blocks.len(), "blocks": blocks}))
    }

    fn read_pdf(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        let count = self.documents.page_count(&path).ok_or_else(|| ToolError::NotFound(path.clone()))?;
        let from = count_arg(args, "from_page", 1, usize::MAX).max(1);
        let most = count_arg(args, "pages", 5, 10).max(1);
        let mut used = 0;
        let mut pages = Vec::new();
        for page in (from..=count).take(most) {
            let text = self.documents.page_text(&path, page - 1).unwrap_or_default();
            if used > 0 && used + text.len() > READ_BUDGET {
                break;
            }
            used += text.len();
            pages.push(json!({"page": page, "text": text}));
        }
        Ok(json!({"path": path, "title": title_of(&path), "page_count": count, "pages": pages}))
    }

    fn outline(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        let headings: Vec<Value> = if kind_of(&path) == "pdf" {
            self.documents.page_count(&path).ok_or_else(|| ToolError::NotFound(path.clone()))?;
            self.documents.outline(&path).into_iter().map(|(level, title, page)| json!({"level": level, "title": title, "page": page + 1})).collect()
        } else {
            self.files.read_text(&path).map_err(|_| ToolError::NotFound(path.clone()))?;
            self.index.headings(&path).map_err(failed)?.into_iter().map(|h| json!({"level": h.level, "title": h.text})).collect()
        };
        Ok(json!({"path": path, "headings": headings}))
    }

    fn links(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for back in self.index.backlinks(&path).map_err(failed)? {
            *counts.entry(back.source).or_default() += 1;
        }
        let backlinks: Vec<Value> = counts.into_iter().map(|(source, n)| json!({"path": source, "title": title_of(&source), "links": n})).collect();
        let outgoing: Vec<Value> = self
            .index
            .links_from(&path)
            .map_err(failed)?
            .into_iter()
            .filter(|l| l.link.kind != knotes_core::LinkKind::External)
            .map(|l| json!({"target": l.link.target, "path": l.resolved}))
            .collect();
        Ok(json!({"path": path, "backlinks": backlinks, "outgoing": outgoing}))
    }

    fn list_notes(&self, args: &Value) -> Result<Value, ToolError> {
        let limit = count_arg(args, "limit", 30, 100);
        let mut rows = self.index.note_rows().map_err(failed)?;
        if let Some(folder) = args.get("folder").and_then(Value::as_str).map(|f| f.trim_matches('/')).filter(|f| !f.is_empty()) {
            let prefix = format!("{folder}/");
            rows.retain(|r| r.path.starts_with(&prefix));
        }
        if let Some(tag) = args.get("tag").and_then(Value::as_str).map(|t| t.trim_start_matches('#')).filter(|t| !t.is_empty()) {
            let tagged = self.index.notes_with_tag(tag).map_err(failed)?;
            rows.retain(|r| tagged.contains(&r.path));
        }
        match args.get("sort").and_then(Value::as_str) {
            Some("name") => rows.sort_by_key(|r| title_of(&r.path).to_lowercase()),
            _ => rows.sort_by_key(|r| std::cmp::Reverse(r.mtime_ns)),
        }
        let notes: Vec<Value> = rows.into_iter().take(limit).map(|r| json!({"path": r.path, "title": title_of(&r.path), "kind": kind_of(&r.path), "modified_unix": r.mtime_ns / 1_000_000_000})).collect();
        Ok(json!({"notes": notes}))
    }

    fn list_tags(&self) -> Result<Value, ToolError> {
        let mut counts = self.index.tag_counts().map_err(failed)?;
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let tags: Vec<Value> = counts.into_iter().take(200).map(|(tag, count)| json!({"tag": tag, "count": count})).collect();
        Ok(json!({"tags": tags}))
    }
}

/// A required string argument.
fn text_arg(args: &Value, name: &str) -> Result<String, ToolError> {
    args.get(name).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(str::to_owned).ok_or_else(|| ToolError::BadArguments(format!("`{name}` (a string) is needed")))
}

/// An optional count, `default` when missing, at most `most`.
fn count_arg(args: &Value, name: &str, default: usize, most: usize) -> usize {
    args.get(name).and_then(Value::as_u64).map_or(default, |n| (n as usize).min(most))
}

/// A note's title (its file name without `.md`), a PDF's file name.
fn title_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_owned()
}

fn kind_of(path: &str) -> &'static str {
    if path.to_lowercase().ends_with(".pdf") { "pdf" } else { "note" }
}

/// `text` cut at `FRAGMENT` characters.
fn cut(text: &str) -> String {
    match text.char_indices().nth(FRAGMENT) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(e.to_string())
}
