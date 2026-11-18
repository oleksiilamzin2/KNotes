//! Cmd+F in the document open: its occurrences in the note, its embedded notes (ADR 0034) or the PDF, stepped through.

use super::*;

/// An occurrence Cmd+F found (UTF-16): in the note, or in an embedded note's view.
#[derive(Clone)]
pub(super) struct Found {
    view: Option<Retained<NSTextView>>,
    range: (usize, usize),
}

impl VaultWindow {
    /// What the PDF viewer highlights as found, and whether the find bar is over its pages
    /// (probe mode).
    pub fn describe_pdf_find(&self) -> Option<(String, bool)> {
        let viewer = self.pdf_view.borrow();
        let viewer = viewer.as_ref()?;
        Some((viewer.describe_found(), self.find_bar.is_over(&viewer.scroll)))
    }

    /// Opened from a search of the notes' text: the words it found, highlighted, the first in
    /// view.
    pub(super) fn show_found_words(&self, text: &str) {
        let query = self.state.borrow().search.clone();
        let (mode, words) = parse_search(&query);
        if words.is_empty() || mode == SearchMode::Files {
            return;
        }
        let found = knotes_app::found_words(text, words);
        let bounds: Vec<usize> = found.iter().flat_map(|r| [r.start, r.end]).collect();
        let units = utf16_offsets(text, &bounds);
        let ranges: Vec<(usize, usize)> = units.chunks(2).map(|p| (p[0], p[1])).collect();
        self.live_preview.show_found(&self.note_text, &ranges);
    }

    /// Edit → Find… and its neighbours from anywhere in the window.
    pub fn find_in_note(&self, sender: Option<&objc2::runtime::AnyObject>) {
        self.find_action(note_view::finder_tag(sender));
    }

    /// A find action (`NSTextFinderAction`): show the bar, the next or previous occurrence, or
    /// hide the bar.
    pub fn find_action(&self, action: isize) {
        use objc2_app_kit::NSTextFinderAction as A;
        match A(action) {
            A::NextMatch | A::PreviousMatch if self.find_bar.is_shown() || !self.find_bar.text().is_empty() => {
                if !self.find_bar.is_shown() {
                    self.open_find();
                }
                self.find_step(A(action) == A::NextMatch);
            }
            A::HideFindInterface => self.close_find(),
            _ => self.open_find(),
        }
    }

    /// Shows the find bar, its text selected and found again in the note.
    pub(super) fn open_find(&self) {
        self.find_bar.show(true);
        let text = self.find_bar.text();
        if !text.is_empty() {
            self.find_text(&text, true);
        }
    }

    /// Done or Escape: the bar and its highlights go, the note gets the focus back.
    pub fn close_find(&self) {
        self.find_bar.show(false);
        self.pdf_find.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.set_found(Vec::new(), None);
        }
        self.live_preview.clear_found(&self.note_text);
        for (_, view, preview) in self.embeds.finders() {
            preview.clear_found(&view);
        }
        self.find_matches.borrow_mut().clear();
        self.find_current.set(None);
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Finds `query` in the note: every occurrence highlighted; with `jump`, the first one from
    /// the cursor on selected and in view (an edit refinds in place).
    pub(super) fn find_text(&self, query: &str, jump: bool) {
        if self.pdf_view.borrow().is_some() {
            self.find_in_pdf(query, jump);
            return;
        }
        let ignore_case = self.find_bar.ignore_case();
        let found_in = |view: &NSTextView| {
            let text = view.string().to_string();
            let bounds: Vec<usize> = knotes_app::find_all_cased(&text, query, ignore_case).iter().flat_map(|r| [r.start, r.end]).collect();
            utf16_offsets(&text, &bounds).chunks(2).map(|p| (p[0], p[1])).collect::<Vec<(usize, usize)>>()
        };
        let own = found_in(&self.note_text);
        self.live_preview.highlight(&self.note_text, &own);
        // In document order: an embedded note's at its line's place (ADR 0034).
        let mut placed: Vec<(usize, usize, Found)> = own.into_iter().map(|range| (range.0, 0, Found { view: None, range })).collect();
        let lines: HashMap<String, usize> = self.live_preview.embeds(&self.note_text.string()).into_iter().map(|(key, _, range)| (key, range.0)).collect();
        for (key, view, preview) in self.embeds.finders() {
            let ranges = found_in(&view);
            preview.highlight(&view, &ranges);
            let at = lines.get(&key).copied().unwrap_or(usize::MAX);
            placed.extend(ranges.into_iter().enumerate().map(|(i, range)| (at, i + 1, Found { view: Some(view.clone()), range })));
        }
        placed.sort_by_key(|(at, nth, _)| (*at, *nth));
        let cursor = self.note_text.selectedRange().location;
        let current = (!placed.is_empty()).then(|| placed.iter().position(|(at, _, _)| *at >= cursor).unwrap_or(0));
        *self.find_matches.borrow_mut() = placed.into_iter().map(|(_, _, found)| found).collect();
        self.find_current.set(current);
        if jump && let Some(i) = current {
            self.select_match(i);
        }
        self.find_bar.show_count(self.find_current.get(), self.find_matches.borrow().len(), !query.is_empty());
    }

    /// Finds `query` in the PDF shown, its pages read one after another on another thread;
    /// with `jump`, the first place from the page shown on comes into view.
    pub(super) fn find_in_pdf(&self, query: &str, jump: bool) {
        if query.is_empty() {
            self.search_pdf(Vec::new(), jump);
            self.find_bar.show_count(None, 0, false);
            return;
        }
        self.find_bar.show_searching();
        self.search_pdf(vec![query.to_owned()], jump);
    }

    /// A document opened from a content search: Cmd+F's bar shows with the search's first
    /// word, every place found highlighted and the first in view (owner's request); the focus
    /// stays where it was.
    pub(super) fn find_search_word(&self) {
        let query = self.state.borrow().search.clone();
        let (mode, words) = parse_search(&query);
        let Some(word) = words.split_whitespace().next().filter(|_| mode != SearchMode::Files).map(str::to_owned) else { return };
        self.find_bar.show_with(&word);
        self.find_text(&word, true);
    }

    /// Finds `words` in the PDF shown, its pages read one after another on another thread;
    /// with `jump`, the first place from the page shown on comes into view; the find bar counts
    /// them.
    pub(super) fn search_pdf(&self, words: Vec<String>, jump: bool) {
        self.pdf_find.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.set_found(Vec::new(), None);
        }
        self.find_current.set(None);
        let (Some(path), Some(vault), Some(mtm)) = (self.pdf_path.borrow().clone(), self.vault(), MainThreadMarker::new()) else { return };
        if words.is_empty() {
            return;
        }
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        *self.pdf_find.borrow_mut() = std::sync::Arc::clone(&cancelled);
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let ignore_case = self.find_bar.ignore_case();
        std::thread::spawn(move || {
            let file = vault.join(&path);
            let found = words.iter().try_fold(Vec::new(), |mut all, word| {
                all.extend(knotes_pdf::find_in_pdf_cased(&file, word, ignore_case, &cancelled)?);
                Some(all)
            });
            let found = found.map(|mut all| {
                all.sort_by_key(|(page, range)| (*page, range.start));
                all.dedup_by(|b, a| a.0 == b.0 && b.1.start < a.1.end);
                all
            });
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                // A newer search, or the bar closed: this one is stale.
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let (Some(window), Some(found)) = (target.get(mtm).upgrade(), found) {
                    window.pdf_found(&path, found, jump);
                }
            });
        });
    }

    /// The places a search found in the PDF at `path`, shown if it is still the one open: the
    /// first from the page shown on is the current one.
    pub(super) fn pdf_found(&self, path: &str, found: Vec<(usize, std::ops::Range<usize>)>, jump: bool) {
        if self.pdf_path.borrow().as_deref() != Some(path) {
            return;
        }
        let total = found.len();
        let current = {
            let viewer = self.pdf_view.borrow();
            let Some(viewer) = viewer.as_ref() else { return };
            let page = viewer.page().0.saturating_sub(1);
            let current = found.iter().position(|(p, _)| *p >= page).or((total > 0).then_some(0));
            viewer.set_found(found, current);
            current
        };
        self.find_current.set(current);
        if jump && let (Some(i), Some(viewer)) = (current, self.pdf_view.borrow().as_ref()) {
            viewer.show_found(i);
        }
        self.find_bar.show_count(current, total, true);
    }

    /// The next (or previous) occurrence, round the note.
    pub(super) fn find_step(&self, forward: bool) {
        if let Some(n) = self.pdf_view.borrow().as_ref().map(PdfViewer::found_len) {
            let Some(i) = knotes_app::step_found(self.find_current.get(), n, forward) else { return };
            self.find_current.set(Some(i));
            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                viewer.show_found(i);
            }
            self.find_bar.show_count(Some(i), n, true);
            return;
        }
        let n = self.find_matches.borrow().len();
        let Some(i) = knotes_app::step_found(self.find_current.get(), n, forward) else { return };
        self.find_current.set(Some(i));
        self.select_match(i);
        self.find_bar.show_count(Some(i), n, true);
    }

    /// Selects occurrence `i`, scrolls it into view and points at it.
    pub(super) fn select_match(&self, i: usize) {
        let Some(found) = self.find_matches.borrow().get(i).cloned() else { return };
        let range = objc2_foundation::NSRange::new(found.range.0, found.range.1 - found.range.0);
        // In an embedded note: its view scrolls the note's, being inside it.
        let view = found.view.unwrap_or_else(|| Retained::into_super(self.note_text.clone()));
        view.setSelectedRange(range);
        view.scrollRangeToVisible(range);
        view.showFindIndicatorForRange(range);
    }
}
