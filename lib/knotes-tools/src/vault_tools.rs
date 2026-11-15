use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use knotes_core::{BlockHit, ContentQuery, DocumentPages, FolderRulesStore, MeaningSearch, NoteParser, Span, TitleSearch, VaultReader, Within};
use knotes_md::MarkdownParser;
use knotes_search::TitleIndex;
use knotes_spaces::{Space, Spaces};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

use crate::{Closeness, ToolError, ToolIndex, ToolSpec, day_start, specs, utc_minute};

/// A fragment's text past this many characters is cut.
const FRAGMENT: usize = 400;
/// A read stops past this many characters, at a block's or page's end.
const READ_BUDGET: usize = 24_000;
/// The tag that keeps a note from the AI (ADR 0004).
pub const NO_AI_TAG: &str = "no-ai";

/// What the user keeps from the AI: the notes tagged `#no-ai`, and the folders whose
/// `.knotes.json` sets `ai` (paths composed).
#[derive(Default)]
struct Kept {
    notes: HashSet<String>,
    folders: HashMap<String, bool>,
    /// The vault's spaces (ADR 0030), read from the same rules files.
    spaces: Spaces,
}

impl Kept {
    /// Whether `path` is kept: tagged, or under a folder whose nearest setting is `"ai": false`.
    fn holds(&self, path: &str) -> bool {
        let path = composed(path);
        if self.notes.contains(&path) {
            return true;
        }
        let mut folder = path.as_str();
        loop {
            folder = folder.rsplit_once('/').map_or("", |(parent, _)| parent);
            if let Some(&shared) = self.folders.get(folder) {
                return !shared;
            }
            if folder.is_empty() {
                return false;
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.folders.values().all(|&shared| shared)
    }
}

/// How many linked documents `related` gives each way.
const RELATED: usize = 30;
/// How many documents near in meaning `related` gives.
const NEAR: usize = 10;
/// Which documents near in meaning `related` gives: one note to another stands out more than a
/// query to a block, so the query's thresholds gave 4.5 notes a note on the benchmark's vault,
/// a washing machine beside a car's insurance and tyres; these give 2.6 with e5 and 2.7 with
/// EmbeddingGemma 2, mostly what is about the same (2026-11-15).
const NEAR_DOCUMENT: Closeness = Closeness { near_enough: 2.2, behind_nearest: 1.2 };
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
    /// What the user keeps from the AI, and when it was looked up.
    kept: RefCell<Option<(Instant, Rc<Kept>)>>,
    /// Search by meaning, merged into search's finds when there (ADR 0028), and which of its
    /// finds are near enough to give.
    meaning: Option<(Box<dyn MeaningSearch>, Closeness)>,
    /// The space the app's window works in (ADR 0030).
    space: RefCell<Option<String>>,
}

impl<I: ToolIndex, F: VaultReader + FolderRulesStore, D: DocumentPages> VaultTools<I, F, D> {
    pub fn new(index: I, files: F, documents: D) -> Self {
        let titles = RefCell::new((Instant::now(), titles_of(&index)));
        Self { index, files, documents, titles, overview: RefCell::new(None), kept: RefCell::new(None), meaning: None, space: RefCell::new(None) }
    }

    /// Search finds blocks by meaning too, merged with those by words (ADR 0028), the ones not
    /// near enough left out.
    pub fn with_meaning(mut self, meaning: Box<dyn MeaningSearch>, closeness: Closeness) -> Self {
        self.meaning = Some((meaning, closeness));
        self
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
        let kept = self.kept();
        let text = describe(&self.index, &kept, &self.tag_counts(&kept));
        *self.overview.borrow_mut() = Some((Instant::now(), text.clone()));
        text
    }

    /// Whether the user keeps `path` from the AI: a note tagged `#no-ai`, or in a folder whose
    /// `.knotes.json` says `"ai": false` (with its subfolders, unless one says `"ai": true`).
    pub fn excluded(&self, path: &str) -> bool {
        self.kept().holds(path)
    }

    /// What the user keeps from the AI, looked up again when older than `TITLES_LAST`.
    fn kept(&self) -> Rc<Kept> {
        if let Some((when, kept)) = self.kept.borrow().as_ref()
            && when.elapsed() <= TITLES_LAST
        {
            return Rc::clone(kept);
        }
        let kept = Rc::new(self.look_up_kept());
        *self.kept.borrow_mut() = Some((Instant::now(), Rc::clone(&kept)));
        kept
    }

    fn look_up_kept(&self) -> Kept {
        let notes = self.index.notes_with_tag(NO_AI_TAG).unwrap_or_default().iter().map(|p| composed(p)).collect();
        let mut folders: BTreeSet<String> = BTreeSet::from([String::new()]);
        for entry in self.index.catalog().unwrap_or_default() {
            let mut path = entry.path.as_str();
            while let Some((folder, _)) = path.rsplit_once('/') {
                folders.insert(folder.to_string());
                path = folder;
            }
        }
        let list: Vec<&str> = folders.iter().map(String::as_str).collect();
        let rules = self.files.read_rules(&list).unwrap_or_default();
        let spaces = Spaces::from_texts(&rules);
        let folders = rules
            .into_iter()
            .filter_map(|(folder, text)| Some((composed(&folder), serde_json::from_str::<Value>(&text).ok()?.get("ai")?.as_bool()?)))
            .collect();
        Kept { notes, folders, spaces }
    }

    /// The space a tool's `space` argument names, by its name (any case) or id; none when not
    /// given.
    fn space_arg(&self, args: &Value, kept: &Kept) -> Result<Option<Space>, ToolError> {
        let Some(asked) = args.get("space").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
        let lower = asked.to_lowercase();
        let found = kept.spaces.list().iter().find(|s| s.id == asked || s.name.to_lowercase() == lower);
        match found {
            Some(space) => Ok(Some(space.clone())),
            None if kept.spaces.is_empty() => Err(ToolError::BadArguments(format!("no space {asked:?}: the vault has no spaces"))),
            None => Err(ToolError::BadArguments(format!("no space {asked:?}; the vault's spaces: {}", kept.spaces.list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")))),
        }
    }

    /// The window's space (ADR 0030), whose finds search gives first; none outside the app.
    pub fn set_space(&self, id: Option<String>) {
        *self.space.borrow_mut() = id;
    }

    /// The tags with how many shared notes have each (the marking tag left out).
    fn tag_counts(&self, kept: &Kept) -> Vec<(String, usize)> {
        let counts = self.index.tag_counts().unwrap_or_default().into_iter().filter(|(tag, _)| tag != NO_AI_TAG);
        if kept.is_empty() {
            return counts.collect();
        }
        counts
            .filter_map(|(tag, _)| {
                let shared = self.index.notes_with_tag(&tag).unwrap_or_default().iter().filter(|p| !kept.holds(p)).count();
                (shared > 0).then_some((tag, shared))
            })
            .collect()
    }

    /// `Err(Excluded)` for a document the user keeps from the AI.
    fn shared(&self, path: &str) -> Result<(), ToolError> {
        if self.excluded(path) { Err(ToolError::Excluded(path.to_string())) } else { Ok(()) }
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
            "related" => self.related(args),
            "list_notes" => self.list_notes(args),
            "list_tasks" => self.list_tasks(args),
            "list_tags" => self.list_tags(),
            _ => Err(ToolError::Unknown(name.to_owned())),
        }
    }

    fn search(&self, args: &Value) -> Result<Value, ToolError> {
        let asked = text_arg(args, "query")?;
        let query = without_question_words(&asked);
        let (offset, limit) = paging(args, 10, 30);
        // One more than the page, to tell whether more follow.
        let wanted = offset + limit + 1;
        let kept = self.kept();
        // A space asked for (ADR 0030): every way of finding keeps to its documents.
        let space = self.space_arg(args, &kept)?;
        let in_space: Option<Vec<String>> = space.as_ref().map(|space| {
            let catalog = self.index.catalog().unwrap_or_default();
            catalog.into_iter().map(|e| e.path).filter(|p| kept.spaces.of(p).is_some_and(|s| s.id == space.id)).collect()
        });
        let find = |text: &str, within: Within, scope: Option<&[String]>| {
            let both: Option<Vec<String>> = match (scope, in_space.as_deref()) {
                (Some(scope), Some(space)) => Some(scope.iter().filter(|p| space.contains(p)).cloned().collect()),
                _ => None,
            };
            let scope = both.as_deref().or(scope).or(in_space.as_deref());
            self.index.search(&ContentQuery { text, within, prefixes: true, scope, limit: wanted }).map_err(failed)
        };
        let mut hits = Hits::default();
        // Fragments holding every word, then (while too few) ever looser ways: the notes holding
        // them across their blocks; the documents whose title or aliases hold them; the same by
        // the words' stems; the documents linked with one a word names, holding the other words.
        hits.add("block", find(&query, Within::Block, None)?, None, true);
        if hits.0.len() < wanted {
            hits.add("note", find(&query, Within::Note, None)?, None, false);
        }
        let named = self.named();
        if hits.0.len() < wanted {
            let words: Vec<String> = query.split_whitespace().map(name_form).collect();
            let titled: Vec<BlockHit> = named
                .iter()
                .filter(|(path, names)| words.iter().all(|w| names.iter().any(|n| n.starts_with(w.as_str()))) && in_space.as_ref().is_none_or(|space| space.contains(path)))
                .map(|(path, _)| title_hit(path))
                .collect();
            hits.add("title", titled, None, false);
        }
        let stemmed = stems(&query);
        if hits.0.len() < wanted && stemmed != query {
            hits.add("stem", find(&stemmed, Within::Block, None)?, None, false);
            hits.add("stem", find(&stemmed, Within::Note, None)?, None, false);
        }
        let words: Vec<&str> = query.split_whitespace().collect();
        if hits.0.len() < wanted && words.len() > 1 {
            for (i, word) in words.iter().enumerate() {
                let word = name_form(word);
                if word.chars().count() < 3 {
                    continue;
                }
                let rest = stems(&words.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, w)| *w).collect::<Vec<_>>().join(" "));
                for (path, _) in named.iter().filter(|(_, names)| names.iter().any(|n| n.starts_with(word.as_str()))).take(5) {
                    let mut near = vec![path.clone()];
                    near.extend(self.index.links_from(path).unwrap_or_default().into_iter().filter_map(|l| l.resolved));
                    near.extend(self.index.backlinks(path).unwrap_or_default().into_iter().map(|b| b.source));
                    hits.add("linked", find(&rest, Within::Note, Some(&near))?, Some(path.clone()), false);
                }
            }
        }
        // Last, for three words or more: all but one of them (by their stems), the one left out
        // named (`partial`, `missing`): a word in another form or another word altogether.
        if hits.0.len() < wanted && words.len() > 2 {
            for (i, word) in words.iter().enumerate() {
                let rest = stems(&words.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, w)| *w).collect::<Vec<_>>().join(" "));
                hits.add("partial", find(&rest, Within::Note, None)?, Some(format!("missing:{word}")), false);
            }
        }
        // Finds by meaning beside them, those standing out too little left out (what the vault
        // does not hold would fill the page with unrelated notes), the two lists merged; each
        // block found by meaning says how far it stood out. A failing search by meaning (its
        // helper gone, no model) leaves search by words as it is.
        // Whether a document the AI may read holds the words (or their stems): told when none
        // does, so the model can tell a search worth rewording from one with nothing to find.
        let by_words = hits.0.iter().any(|(_, hit, _)| !kept.holds(&hit.path));
        let mut standing: HashMap<(String, u32), f64> = HashMap::new();
        if let Some((meaning, closeness)) = &self.meaning
            && let Ok(near) = meaning.nearest(&asked, wanted, in_space.as_deref())
        {
            let near = closeness.keep(near);
            standing = near.iter().map(|h| ((h.path.clone(), h.ord), -h.score)).collect();
            hits.0 = merged(std::mem::take(&mut hits.0), near);
        }
        hits.0.retain(|(_, hit, via)| !kept.holds(&hit.path) && via.as_deref().is_none_or(|v| v.starts_with("missing:") || !kept.holds(v)));
        // The window's space first, the rest in the order found (ADR 0030).
        if space.is_none()
            && let Some(id) = self.space.borrow().as_deref()
        {
            hits.0.sort_by_key(|(_, hit, _)| kept.spaces.of(&hit.path).is_none_or(|s| s.id != id));
        }
        let (hits, next) = page(hits.0, offset, limit);
        let results: Vec<Value> = hits
            .into_iter()
            .map(|(found, hit, via)| {
                let text = self.fragment(&hit);
                let mut result = json!({"path": hit.path, "title": title_of(&hit.path), "kind": kind_of(&hit.path), "modified": self.modified(&hit.path), "heading": hit.heading, "block": hit.ord, "text": text, "match": found});
                if let Some(s) = standing.get(&(hit.path.clone(), hit.ord)) {
                    result["closeness"] = json!((s * 10.0).round() / 10.0);
                }
                if let Some(space) = kept.spaces.of(&hit.path) {
                    result["space"] = json!(space.name);
                }
                // A partial find names the word it lacks; a linked one, the document it came through.
                if let Some((key, value)) = via.as_deref().map(|v| v.strip_prefix("missing:").map_or(("via", v), |w| ("missing", w))) {
                    result[key] = json!(value);
                }
                result
            })
            .collect();
        let mut found = json!({"results": results, "next_offset": next});
        if !by_words {
            found["words_found"] = json!(false);
        }
        Ok(found)
    }

    /// The documents with the words of their titles and aliases (composed, lower case), for
    /// the words of a query to name them.
    fn named(&self) -> Vec<(String, Vec<String>)> {
        let kept = self.kept();
        self.index
            .catalog()
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| !kept.holds(&entry.path))
            .map(|entry| {
                let mut names: Vec<String> = Vec::new();
                for name in std::iter::once(title_of(&entry.path)).chain(entry.aliases) {
                    names.extend(name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(name_form));
                }
                (entry.path, names)
            })
            .collect()
    }

    fn find_by_title(&self, args: &Value) -> Result<Value, ToolError> {
        let query = text_arg(args, "query")?;
        let (offset, limit) = paging(args, 10, 30);
        if self.titles.borrow().0.elapsed() > TITLES_LAST {
            *self.titles.borrow_mut() = (Instant::now(), titles_of(&self.index));
        }
        let kept = self.kept();
        let found: Vec<_> = self.titles.borrow().1.search_titles(&query, DEEPEST + 1).into_iter().filter(|h| !kept.holds(&h.path)).take(offset + limit + 1).collect();
        let (hits, next) = page(found, offset, limit);
        let results: Vec<Value> = hits.into_iter().map(|h| json!({"path": h.path, "title": title_of(&h.path), "kind": kind_of(&h.path)})).collect();
        Ok(json!({"results": results, "next_offset": next}))
    }

    /// When the document at `path` last changed, to the minute in UTC, if it is indexed.
    fn modified(&self, path: &str) -> Option<String> {
        self.index.modified(path).ok().flatten().map(|ns| utc_minute(ns.div_euclid(1_000_000_000)))
    }

    fn read_note(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        self.shared(&path)?;
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
        Ok(json!({"path": path, "title": title_of(&path), "modified": self.modified(&path), "total_blocks": parsed.blocks.len(), "blocks": blocks, "next_block": next}))
    }

    fn read_pdf(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        self.shared(&path)?;
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
        self.shared(&path)?;
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
        self.shared(&path)?;
        let kept = self.kept();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for back in self.index.backlinks(&path).map_err(failed)? {
            if !kept.holds(&back.source) {
                *counts.entry(back.source).or_default() += 1;
            }
        }
        let backlinks: Vec<Value> = counts.into_iter().map(|(source, n)| json!({"path": source, "title": title_of(&source), "links": n})).collect();
        let outgoing: Vec<Value> = self
            .index
            .links_from(&path)
            .map_err(failed)?
            .into_iter()
            .filter(|l| l.link.kind != knotes_core::LinkKind::External && !l.resolved.as_deref().is_some_and(|p| kept.holds(p)))
            .map(|l| json!({"target": l.link.target, "path": l.resolved}))
            .collect();
        Ok(json!({"path": path, "backlinks": backlinks, "outgoing": outgoing}))
    }

    fn related(&self, args: &Value) -> Result<Value, ToolError> {
        let path = text_arg(args, "path")?;
        self.shared(&path)?;
        let text = self.text_of(&path).ok_or_else(|| ToolError::NotFound(path.clone()))?;
        let kept = self.kept();
        let mut to: Vec<String> = Vec::new();
        for link in self.index.links_from(&path).map_err(failed)? {
            if let Some(target) = link.resolved
                && target != path
                && !to.contains(&target)
                && !kept.holds(&target)
            {
                to.push(target);
            }
        }
        let mut from: Vec<String> = Vec::new();
        for back in self.index.backlinks(&path).map_err(failed)? {
            if back.source != path && !from.contains(&back.source) && !kept.holds(&back.source) {
                from.push(back.source);
            }
        }
        // The notes near it in meaning that its links do not show, each once as its nearest
        // block, those merely near the subject left out (when the app searches by meaning).
        let near = self.meaning.as_ref().map(|(meaning, _)| {
            let asked = (NEAR + to.len() + from.len()).min(DEEPEST);
            NEAR_DOCUMENT
                .keep(meaning.near_document(&path, asked).unwrap_or_default())
                .into_iter()
                .filter(|h| !to.contains(&h.path) && !from.contains(&h.path) && !kept.holds(&h.path))
                .take(NEAR)
                .map(|h| json!({"path": h.path, "title": title_of(&h.path), "closeness": (-h.score * 10.0).round() / 10.0, "text": cut_to(&self.fragment(&h), 200)}))
                .collect::<Vec<Value>>()
        });
        let links_to: Vec<Value> = to
            .iter()
            .take(RELATED)
            .map(|p| json!({"path": p, "title": title_of(p), "kind": kind_of(p), "start": self.text_of(p).map(|t| cut_to(&body_of(&t), 200)).unwrap_or_default()}))
            .collect();
        let title = composed(&title_of(&path)).to_lowercase();
        let linked_from: Vec<Value> = from
            .iter()
            .take(RELATED)
            .map(|p| {
                let line = self.text_of(p).and_then(|t| t.lines().find(|l| composed(l).to_lowercase().contains(&title)).map(|l| cut_to(l.trim(), 300))).unwrap_or_default();
                json!({"path": p, "title": title_of(p), "line": line})
            })
            .collect();
        let mut related = json!({"path": path, "title": title_of(&path), "text": cut_to(&body_of(&text), 600), "links_to": links_to, "linked_from": linked_from, "more_to": to.len().saturating_sub(RELATED), "more_from": from.len().saturating_sub(RELATED)});
        if let Some(near) = near {
            related["near_in_meaning"] = json!(near);
        }
        Ok(related)
    }

    /// A found block's text, cut (its end within the note's text: the vectors may be older).
    fn fragment(&self, hit: &BlockHit) -> String {
        self.index.note_text(&hit.path).ok().flatten().and_then(|t| t.get(hit.span.start as usize..(hit.span.end as usize).min(t.len())).map(cut)).unwrap_or_default()
    }

    fn list_tasks(&self, args: &Value) -> Result<Value, ToolError> {
        let offset = count_arg(args, "offset", 0, usize::MAX);
        let limit = count_arg(args, "limit", 30, 100).max(1);
        let state = args.get("state").and_then(Value::as_str).unwrap_or("open");
        let wanted = |done: bool| match state {
            "done" => done,
            "all" => true,
            _ => !done,
        };
        let kept = self.kept();
        let mut rows = self.index.note_rows().map_err(failed)?;
        rows.retain(|r| kind_of(&r.path) == "note" && !kept.holds(&r.path));
        if let Some(folder) = args.get("folder").and_then(Value::as_str).map(|f| composed(f.trim_matches('/'))).filter(|f| !f.is_empty()) {
            let prefix = format!("{folder}/");
            rows.retain(|r| composed(&r.path).starts_with(&prefix));
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r.mtime_ns));
        let mut notes = Vec::new();
        for row in rows {
            let Some(text) = self.text_of(&row.path) else { continue };
            let tasks = tasks_in(&text, &wanted);
            if !tasks.is_empty() {
                notes.push(json!({"path": row.path, "title": title_of(&row.path), "modified": utc_minute(row.mtime_ns.div_euclid(1_000_000_000)), "tasks": tasks}));
            }
        }
        let total = notes.len();
        let next = (offset + limit < total).then_some(offset + limit);
        let notes: Vec<Value> = notes.into_iter().skip(offset).take(limit).collect();
        Ok(json!({"notes": notes, "total": total, "next_offset": next}))
    }

    /// A document's text: a note's as indexed (else from its file), a PDF's pages.
    fn text_of(&self, path: &str) -> Option<String> {
        if let Some(text) = self.index.note_text(path).ok().flatten() {
            return Some(text);
        }
        if kind_of(path) == "pdf" {
            let count = self.documents.page_count(path)?;
            return Some((0..count.min(3)).filter_map(|p| self.documents.page_text(path, p)).collect::<Vec<_>>().join("\n"));
        }
        self.files.read_text(path).ok()
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
        let kept = self.kept();
        let space = self.space_arg(args, &kept)?;
        let mut rows = self.index.note_rows().map_err(failed)?;
        rows.retain(|r| !kept.holds(&r.path) && space.as_ref().is_none_or(|space| kept.spaces.of(&r.path).is_some_and(|s| s.id == space.id)));
        let seconds = |ns: i64| ns.div_euclid(1_000_000_000);
        if let Some(after) = after {
            rows.retain(|r| seconds(r.mtime_ns) >= after);
        }
        if let Some(before) = before {
            rows.retain(|r| seconds(r.mtime_ns) < before + 86_400);
        }
        // Compared composed: macOS writes some names decomposed.
        if let Some(folder) = args.get("folder").and_then(Value::as_str).map(|f| composed(f.trim_matches('/'))).filter(|f| !f.is_empty()) {
            let prefix = format!("{folder}/");
            rows.retain(|r| composed(&r.path).starts_with(&prefix));
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
        let notes: Vec<Value> = rows
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|r| {
                let mut note = json!({"path": r.path, "title": title_of(&r.path), "kind": kind_of(&r.path), "modified": utc_minute(seconds(r.mtime_ns))});
                if let Some(space) = kept.spaces.of(&r.path) {
                    note["space"] = json!(space.name);
                }
                note
            })
            .collect();
        let next = (offset + limit < total).then_some(offset + limit);
        Ok(json!({"notes": notes, "total": total, "next_offset": next}))
    }

    fn list_tags(&self) -> Result<Value, ToolError> {
        let mut counts = self.tag_counts(&self.kept());
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let tags: Vec<Value> = counts.into_iter().take(200).map(|(tag, count)| json!({"tag": tag, "count": count})).collect();
        Ok(json!({"tags": tags}))
    }
}

/// How `index`'s vault looks, in a few sentences: without what the user keeps from the AI,
/// with the tags as `tags` counts them.
fn describe(index: &impl ToolIndex, kept: &Kept, tags: &[(String, usize)]) -> String {
    let catalog: Vec<_> = index.catalog().unwrap_or_default().into_iter().filter(|e| !kept.holds(&e.path)).collect();
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
    // The spaces (ADR 0030), each with the folders whose rules put them in it.
    if !kept.spaces.is_empty() {
        let mut all: BTreeSet<&str> = BTreeSet::new();
        for entry in &catalog {
            let mut path = entry.path.as_str();
            while let Some((folder, _)) = path.rsplit_once('/') {
                all.insert(folder);
                path = folder;
            }
        }
        let listed: Vec<String> = kept
            .spaces
            .list()
            .iter()
            .map(|space| {
                let folders: Vec<&str> = all.iter().copied().filter(|f| kept.spaces.own(f).flatten().is_some_and(|s| s.id == space.id)).collect();
                format!("{} ({})", space.name, folders.join(", "))
            })
            .collect();
        text.push_str(&format!(" Spaces (named sets of folders the user works in; search and list_notes take one as `space`): {}.", listed.join(", ")));
    }
    let mut tags = tags.to_vec();
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

/// Search hits as they are gathered: how each was found, the hit, and the document it was
/// found through (`linked`).
/// A find: how it was found, the block, and the document it came through or the word it lacks.
type Find = (&'static str, BlockHit, Option<String>);

#[derive(Default)]
struct Hits(Vec<Find>);

impl Hits {
    /// Adds `found` hits: every one (`all`), or only those of documents not there yet.
    fn add(&mut self, how: &'static str, found: Vec<BlockHit>, via: Option<String>, all: bool) {
        for hit in found {
            if all || !self.0.iter().any(|(_, h, _)| h.path == hit.path) {
                self.0.push((how, hit, via.clone()));
            }
        }
    }
}

/// Finds by words and by meaning in one list, by reciprocal rank fusion (k = 60): a block's
/// score is the sum of 1 / (60 + its rank) in each list it is in, so one both lists rank high
/// comes first. The first find by words stays first, an exact match never pushed down by
/// meaning. A block in both keeps how words found it; one found only by meaning is `meaning`.
fn merged(words: Vec<Find>, meaning: Vec<BlockHit>) -> Vec<Find> {
    const K: f64 = 60.0;
    let mut found: Vec<(Find, f64)> = Vec::new();
    let mut add = |entry: Find, rank: usize| {
        let score = 1.0 / (K + rank as f64 + 1.0);
        match found.iter_mut().find(|(e, _)| e.1.path == entry.1.path && e.1.ord == entry.1.ord) {
            Some((_, total)) => *total += score,
            None => found.push((entry, score)),
        }
    };
    let first = words.first().map(|(_, hit, _)| (hit.path.clone(), hit.ord));
    for (rank, entry) in words.into_iter().enumerate() {
        add(entry, rank);
    }
    for (rank, hit) in meaning.into_iter().enumerate() {
        add(("meaning", hit, None), rank);
    }
    for (entry, score) in &mut found {
        if first.as_ref().is_some_and(|(path, ord)| entry.1.path == *path && entry.1.ord == *ord) {
            *score = f64::INFINITY;
        }
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.into_iter().map(|(entry, _)| entry).collect()
}

/// A document found by its title: its start, no fragment.
fn title_hit(path: &str) -> BlockHit {
    BlockHit { path: path.to_string(), ord: 0, span: Span { start: 0, end: 0 }, heading: None, score: 0.0 }
}

/// `text` composed (macOS writes some file names decomposed).
fn composed(text: &str) -> String {
    text.nfc().collect()
}

/// A note's text without its frontmatter and its first heading line.
fn body_of(text: &str) -> String {
    let mut rest = text;
    if let Some(after) = rest.strip_prefix("---\n")
        && let Some(end) = after.find("\n---")
    {
        rest = after[end + 4..].trim_start_matches('\n');
    }
    let rest = rest.trim_start();
    let rest = if rest.starts_with('#') { rest.split_once('\n').map_or("", |(_, r)| r) } else { rest };
    rest.trim().to_string()
}

/// `text` cut at `most` characters.
fn cut_to(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// The task items (`- [ ]`, `* [x]`, `1. [ ]`) of `text` that `wanted` keeps (by whether they
/// are done), each with its line (from 1), its words and the heading it is under.
fn tasks_in(text: &str, wanted: &dyn Fn(bool) -> bool) -> Vec<Value> {
    let mut heading: Option<String> = None;
    let mut tasks = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(title) = trimmed.strip_prefix('#') {
            heading = Some(title.trim_start_matches('#').trim().to_string());
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
            .or_else(|| trimmed.split_once(". ").filter(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).map(|(_, rest)| rest));
        let Some(item) = item else { continue };
        let (done, words) = if let Some(words) = item.strip_prefix("[ ] ") {
            (false, words)
        } else if let Some(words) = item.strip_prefix("[x] ").or_else(|| item.strip_prefix("[X] ")) {
            (true, words)
        } else {
            continue;
        };
        if wanted(done) {
            tasks.push(json!({"line": i + 1, "text": words.trim(), "done": done, "heading": heading}));
        }
    }
    tasks
}

/// A word as names are compared: composed (macOS writes some file names decomposed), lower case.
fn name_form(word: &str) -> String {
    word.nfc().flat_map(char::to_lowercase).collect()
}

/// `query` with its longer Cyrillic words cut to their stems (two letters off a word of six or
/// more, one off a word of five), so other forms of a word match: "помилка" finds "помилок",
/// "Софійка" finds "Софійці", "Марка" finds "Марко".
fn stems(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let n = word.chars().count();
            let cyrillic = word.chars().all(|c| ('\u{400}'..='\u{4ff}').contains(&c) || c == '\'' || c == '’');
            let off = match n {
                _ if !cyrillic => 0,
                5 => 1,
                n if n >= 6 => 2,
                _ => 0,
            };
            word.chars().take(n - off).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Question and function words (Ukrainian, Russian, English): no note holds a question's "when".
const QUESTION_WORDS: &[&str] = &[
    "коли", "що", "як", "де", "куди", "звідки", "хто", "чому", "навіщо", "скільки", "який", "яка", "яке", "які", "якого", "якій", "яких", "чи", "та", "але", "або",
    "для", "про", "від", "при", "це", "цей", "ця", "ці", "той", "так", "ще", "вже", "мій", "моя", "моє", "мої", "наш", "наша", "наше", "наші", "мені", "мене", "ми",
    "був", "була", "було", "були", "буде", "когда", "что", "как", "где", "кто", "почему", "зачем", "сколько", "какой", "какая", "какое", "какие", "или", "это",
    "мой", "мне", "меня", "был", "была", "было", "when", "what", "how", "where", "who", "why", "which", "the", "is", "are", "was", "were", "of", "to", "in", "on",
    "for", "and", "or", "do", "does", "did", "my", "our",
    // Prepositions, conjunctions, particles and pronouns: as starts of words ("за" finds "заміна",
    // "закрити") they bring in notes that have nothing to do with the query.
    "і", "й", "в", "у", "з", "із", "зі", "за", "на", "до", "по", "о", "об", "під", "над", "між", "через", "без", "біля", "після", "перед", "якщо", "щоб", "бо",
    "а", "не", "ні", "ж", "же", "б", "би", "то", "хоч", "ось", "там", "тут", "я", "ти", "він", "вона", "воно", "вони", "ви", "його", "її", "їх", "їм", "нам", "вам",
    "кого", "кому", "чим", "чого", "и", "во", "с", "со", "к", "ко", "от", "из", "ни", "бы", "если", "нет", "a", "an", "at", "by", "be", "as", "it", "if", "so",
    "no", "not", "from", "with", "about", "we", "you", "i", "me", "us", "they", "them", "he", "she", "his", "her", "their", "this", "that", "there", "into", "can",
];

/// `query` without its question and function words, unless it holds nothing else.
pub fn without_question_words(query: &str) -> String {
    let kept: Vec<&str> = query.split_whitespace().filter(|w| !QUESTION_WORDS.contains(&name_form(w).as_str())).collect();
    if kept.is_empty() { query.to_string() } else { kept.join(" ") }
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
