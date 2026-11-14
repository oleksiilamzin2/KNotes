use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use knotes_core::{ContentQuery, DocumentPages, NoteParser, TitleSearch, VaultReader, Within};
use knotes_md::MarkdownParser;
use knotes_search::TitleIndex;
use serde_json::{Value, json};

use crate::{ToolError, ToolIndex, ToolSpec, day_start, specs, utc_minute};

/// A fragment's text past this many characters is cut.
const FRAGMENT: usize = 400;
/// A read stops past this many characters, at a block's or page's end.
const READ_BUDGET: usize = 24_000;
/// How deep search and title search page (offset and page together); past it, a narrower
/// query does better.
const DEEPEST: usize = 300;
/// How old the titles may get before they are listed again (notes come and go meanwhile).
const TITLES_LAST: Duration = Duration::from_secs(10);

/// The vault's read-only tools (ADR 0027) over its index, its files and its PDFs' pages.
pub struct VaultTools<I, F, D> {
    index: I,
    files: F,
    documents: D,
    /// The titles to find documents by, and when they were listed.
    titles: RefCell<(Instant, TitleIndex)>,
    /// The vault in a few words, and when it was described.
    overview: RefCell<Option<(Instant, String)>>,
}

impl<I: ToolIndex, F: VaultReader, D: DocumentPages> VaultTools<I, F, D> {
    pub fn new(index: I, files: F, documents: D) -> Self {
        let titles = RefCell::new((Instant::now(), titles_of(&index)));
        Self { index, files, documents, titles, overview: RefCell::new(None) }
    }

    /// The tools' descriptions.
    pub fn specs() -> Vec<ToolSpec> {
        specs()
    }

    /// The vault in a few words, for the model: how many notes and PDFs, its top folders and
    /// tags, and the script its titles are mostly in (so it searches in the notes' language).
    pub fn overview(&self) -> String {
        if let Some((when, text)) = self.overview.borrow().as_ref()
            && when.elapsed() <= TITLES_LAST
        {
            return text.clone();
        }
        let text = describe(&self.index);
        *self.overview.borrow_mut() = Some((Instant::now(), text.clone()));
        text
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
        let (offset, limit) = paging(args, 10, 30);
        // One more than the page, to tell whether more follow.
        let wanted = offset + limit + 1;
        let blocks = self.index.search(&ContentQuery { text: &query, within: Within::Block, prefixes: true, scope: None, limit: wanted }).map_err(failed)?;
        let mut hits: Vec<(&str, _)> = blocks.into_iter().map(|hit| ("block", hit)).collect();
        // Too few fragments hold every word: then the notes holding them across their blocks, each
        // once, with its best block (a question's words are often spread over a note).
        if hits.len() < wanted {
            let notes = self.index.search(&ContentQuery { text: &query, within: Within::Note, prefixes: true, scope: None, limit: wanted }).map_err(failed)?;
            for hit in notes {
                if !hits.iter().any(|(_, h)| h.path == hit.path) {
                    hits.push(("note", hit));
                }
            }
        }
        let (hits, next) = page(hits, offset, limit);
        let results: Vec<Value> = hits
            .into_iter()
            .map(|(found, hit)| {
                let text = self.index.note_text(&hit.path).ok().flatten().and_then(|t| t.get(hit.span.start as usize..hit.span.end as usize).map(cut)).unwrap_or_default();
                json!({"path": hit.path, "title": title_of(&hit.path), "kind": kind_of(&hit.path), "heading": hit.heading, "block": hit.ord, "text": text, "match": found})
            })
            .collect();
        Ok(json!({"results": results, "next_offset": next}))
    }

    fn find_by_title(&self, args: &Value) -> Result<Value, ToolError> {
        let query = text_arg(args, "query")?;
        let (offset, limit) = paging(args, 10, 30);
        if self.titles.borrow().0.elapsed() > TITLES_LAST {
            *self.titles.borrow_mut() = (Instant::now(), titles_of(&self.index));
        }
        let (hits, next) = page(self.titles.borrow().1.search_titles(&query, offset + limit + 1), offset, limit);
        let results: Vec<Value> = hits.into_iter().map(|h| json!({"path": h.path, "title": title_of(&h.path), "kind": kind_of(&h.path)})).collect();
        Ok(json!({"results": results, "next_offset": next}))
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
        let next = from + blocks.len();
        let next = (next < parsed.blocks.len()).then_some(next);
        Ok(json!({"path": path, "title": title_of(&path), "total_blocks": parsed.blocks.len(), "blocks": blocks, "next_block": next}))
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
        let next = from + pages.len();
        let next = (next <= count).then_some(next);
        Ok(json!({"path": path, "title": title_of(&path), "page_count": count, "pages": pages, "next_page": next}))
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
        let offset = count_arg(args, "offset", 0, usize::MAX);
        let limit = count_arg(args, "limit", 30, 100).max(1);
        let day = |name: &str| -> Result<Option<i64>, ToolError> {
            match args.get(name).and_then(Value::as_str) {
                None => Ok(None),
                Some(text) => day_start(text).map(Some).ok_or_else(|| ToolError::BadArguments(format!("`{name}` is a day, YYYY-MM-DD, not {text:?}"))),
            }
        };
        let (after, before) = (day("changed_after")?, day("changed_before")?);
        let mut rows = self.index.note_rows().map_err(failed)?;
        let seconds = |ns: i64| ns.div_euclid(1_000_000_000);
        if let Some(after) = after {
            rows.retain(|r| seconds(r.mtime_ns) >= after);
        }
        if let Some(before) = before {
            rows.retain(|r| seconds(r.mtime_ns) < before + 86_400);
        }
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
        let total = rows.len();
        let notes: Vec<Value> = rows.into_iter().skip(offset).take(limit).map(|r| json!({"path": r.path, "title": title_of(&r.path), "kind": kind_of(&r.path), "modified": utc_minute(seconds(r.mtime_ns))})).collect();
        let next = (offset + limit < total).then_some(offset + limit);
        Ok(json!({"notes": notes, "total": total, "next_offset": next}))
    }

    fn list_tags(&self) -> Result<Value, ToolError> {
        let mut counts = self.index.tag_counts().map_err(failed)?;
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let tags: Vec<Value> = counts.into_iter().take(200).map(|(tag, count)| json!({"tag": tag, "count": count})).collect();
        Ok(json!({"tags": tags}))
    }
}

/// How `index`'s vault looks, in a few sentences.
fn describe(index: &impl ToolIndex) -> String {
    let catalog = index.catalog().unwrap_or_default();
    let pdfs = catalog.iter().filter(|e| kind_of(&e.path) == "pdf").count();
    let notes = catalog.len() - pdfs;
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut text = format!("The vault: {} and {}.", plural(notes, "note", "notes"), plural(pdfs, "PDF", "PDFs"));
    let mut folders: BTreeMap<&str, usize> = BTreeMap::new();
    let mut top = 0;
    for entry in &catalog {
        match entry.path.split_once('/') {
            Some((folder, _)) => *folders.entry(folder).or_default() += 1,
            None => top += 1,
        }
    }
    if !folders.is_empty() {
        let mut folders: Vec<(&str, usize)> = folders.into_iter().collect();
        folders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let listed: Vec<String> = folders.iter().take(25).map(|(f, n)| format!("{f} ({n})")).collect();
        let more = if folders.len() > 25 { format!(" and {} more", folders.len() - 25) } else { String::new() };
        let mut sorted = listed.clone();
        sorted.sort();
        text.push_str(&format!(" Top-level folders (files in each): {}{more}", sorted.join(", ")));
        text.push_str(&if top > 0 { format!("; {top} files at the top.") } else { ".".to_string() });
    }
    let mut tags = index.tag_counts().unwrap_or_default();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if !tags.is_empty() {
        let listed: Vec<String> = tags.iter().take(15).map(|(t, n)| format!("{t} ({n})")).collect();
        text.push_str(&format!(" Most used tags: {}.", listed.join(", ")));
    }
    let cyrillic = catalog.iter().filter(|e| title_of(&e.path).chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c))).count();
    let script = match catalog.len() {
        0 => None,
        n if cyrillic * 2 > n => Some("Cyrillic script (Ukrainian or Russian)"),
        n if cyrillic * 5 < n => Some("Latin script"),
        _ => Some("both Cyrillic and Latin scripts"),
    };
    if let Some(script) = script {
        text.push_str(&format!(" Its titles are mostly in {script}: search with words in the notes' language."));
    }
    text
}

/// The index's documents' titles and aliases, to find them by.
fn titles_of(index: &impl ToolIndex) -> TitleIndex {
    TitleIndex::new(index.catalog().unwrap_or_default().into_iter().map(|e| (e.path, e.aliases)))
}

/// A required string argument.
fn text_arg(args: &Value, name: &str) -> Result<String, ToolError> {
    args.get(name).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(str::to_owned).ok_or_else(|| ToolError::BadArguments(format!("`{name}` (a string) is needed")))
}

/// The `offset` and `limit` of a paged call: the limit `default` when missing, at most
/// `most`; offset and limit together no deeper than `DEEPEST`.
fn paging(args: &Value, default: usize, most: usize) -> (usize, usize) {
    let limit = count_arg(args, "limit", default, most).max(1);
    (count_arg(args, "offset", 0, DEEPEST - limit), limit)
}

/// The page of `items` (fetched up to one past it) at `offset`, and where the next starts
/// when more follow (and the next is not too deep).
fn page<T>(items: Vec<T>, offset: usize, limit: usize) -> (Vec<T>, Option<usize>) {
    let more = items.len() > offset + limit && offset + limit < DEEPEST;
    (items.into_iter().skip(offset).take(limit).collect(), more.then_some(offset + limit))
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
