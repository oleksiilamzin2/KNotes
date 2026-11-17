//! The list's search (ADR 0015): its row, its query sent to the search thread, its results.

use super::*;

impl VaultWindow {
    /// The files/content switch: rewrites the query's operator and searches again.
    pub fn set_search_mode(&self, mode: SearchMode) {
        let text = with_search_mode(&self.search_bar.field.stringValue().to_string(), mode);
        self.search_bar.set_text(&text);
        self.search(&text);
    }

    /// Edit → Search Notes (Cmd+Shift+F): the search shown, its field focused.
    pub fn show_search(&self) {
        self.toolbar_delegate.set_search_shown(true);
        self.search_bar.show(true);
    }

    /// The magnifying glass: shows the search field in the place of the list's name, or hides
    /// it and drops the search.
    pub fn toggle_search(&self) {
        let shown = !self.search_bar.is_shown();
        self.toolbar_delegate.set_search_shown(shown);
        self.search_bar.show(shown);
        if !shown {
            self.search("");
        }
    }

    /// The search field changed: the list shows the results within the selected list.
    pub fn search(&self, query: &str) {
        let (mode, text) = parse_search(query);
        self.search_bar.show_mode(mode);
        // An operator alone (`content:`) searches nothing yet: the list stays.
        self.state.borrow_mut().search = if text.is_empty() { String::new() } else { query.to_owned() };
        if text.is_empty() {
            *self.search_results.borrow_mut() = None;
            self.refresh_list();
        } else {
            *self.search_results.borrow_mut() = None;
            self.refresh_list();
            self.request_search();
        }
    }

    /// Asks the search thread for the results of the current query within the selected list;
    /// they replace the list when they arrive (`apply_search`).
    pub(super) fn request_search(&self) {
        let generation = self.search_generation.get() + 1;
        self.search_generation.set(generation);
        let (query, scope, titles, index_dir) = {
            let state = self.state.borrow();
            let scope = self.search_scope.borrow().clone();
            (state.search.clone(), (scope, self.search_space.borrow().clone()), state.titles.clone(), state.index_dir.clone())
        };
        let (mode, query) = {
            let (mode, text) = parse_search(&query);
            (mode, text.to_owned())
        };
        // Search by meaning asked: its model loads while the query is still being typed.
        if mode == knotes_app::SearchMode::Meaning
            && let Some(embedder) = crate::meaning::embedder()
        {
            embedder.warm();
        }
        if self.search_worker.borrow().is_none() {
            let Some(index_dir) = index_dir else { return };
            let Some(mtm) = MainThreadMarker::new() else { return };
            let target = Arc::new(MainThreadBound::new(self.this.borrow().clone(), mtm));
            let worker = SearchWorker::start(index_dir, move |response| {
                let target = Arc::clone(&target);
                DispatchQueue::main().exec_async(move || {
                    let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                    if let Some(window) = target.get(mtm).upgrade() {
                        window.apply_search(response);
                    }
                });
            });
            *self.search_worker.borrow_mut() = Some(worker);
        }
        if let Some(worker) = self.search_worker.borrow().as_ref() {
            let closeness = crate::meaning::model().unwrap_or(knotes_vectors::MeaningModel::DEFAULT).closeness();
            let (scope, space) = scope;
            worker.request(SearchRequest { generation, mode, query, scope, titles, closeness, space });
        }
    }

    /// Results from the search thread: shown unless a newer query is on its way.
    pub(super) fn apply_search(&self, response: SearchResponse) {
        if response.generation == self.search_generation.get() && !self.state.borrow().search.trim().is_empty() {
            *self.search_results.borrow_mut() = Some(response);
            self.refresh_list();
        }
    }
}
