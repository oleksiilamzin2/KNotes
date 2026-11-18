//! Tabs (ADR 0024): each document a tab, the place in it kept, the tabs saved beside the index.

use super::*;

/// The vault's tabs, beside its index (ADR 0024).
pub(super) const TABS_FILE: &str = "tabs.json";

impl VaultWindow {
    /// The tabs along the toolbar, as they are now.
    pub(super) fn show_tabs(&self) {
        let tabs = self.tabs.borrow();
        // The marks the sidebar's rows of tabs had: another space's dot, and the AI's. (Asked
        // while the window's state is being changed, a tab shows without them until the next.)
        let state = self.state.try_borrow().ok();
        let shown = tabs
            .paths()
            .map(|path| {
                let deck = super::cards::deck_of_tab(path);
                let document = state.as_ref().filter(|_| deck.is_none());
                let space = document.and_then(|state| knotes_app::space_mark(&state.spaces, state.space.as_deref(), path).map(|space| (space.color, space.name.clone())));
                let kept_from_ai = document.is_some_and(|state| state.ai.holds(path));
                let name = deck.map_or_else(|| document_name(path).to_owned(), |deck| deck.rsplit('/').next().unwrap_or(deck).to_owned());
                let symbol = if deck.is_some() { "rectangle.stack" } else if path.to_lowercase().ends_with(".pdf") { "doc.richtext" } else { "doc.text" };
                crate::tab_strip::Tab { path: path.to_owned(), name, symbol, space, kept_from_ai }
            })
            .collect();
        self.tab_strip.set(shown, tabs.current());
    }

    /// Closes `path`'s tab: the document shown stays, or its neighbour's shows, or nothing.
    /// False when it stays open: its note's edits could not be written and the user chose to
    /// stay.
    pub fn close_tab(&self, path: &str) -> bool {
        let Some(i) = self.tabs.borrow().paths().position(|p| p == path) else { return true };
        // The tab shown goes: its note is left first (the tab stays when the user stays). Not
        // borrowed while leaving, which keeps the tab's place.
        let shown = self.tabs.borrow().current_path() == Some(path);
        if shown && !self.leave() {
            return false;
        }
        let after = self.tabs.borrow_mut().close(i);
        match after {
            AfterClose::Show(next) => {
                self.open_note(&next, true);
            }
            AfterClose::Nothing => {
                self.show_nothing();
            }
            AfterClose::Same => {}
        }
        self.save_tabs();
        true
    }

    /// The tab `by` after the one shown (before, when negative), round the tabs' ends.
    pub fn step_tab(&self, by: isize) {
        let next = {
            let tabs = self.tabs.borrow();
            let paths: Vec<&str> = tabs.paths().collect();
            let Some(current) = tabs.current() else { return };
            if paths.len() < 2 {
                return;
            }
            paths[(current as isize + by).rem_euclid(paths.len() as isize) as usize].to_owned()
        };
        self.open_note(&next, true);
    }

    /// The tab of `number` (from 1); 9 is the last, however many there are (as Safari's ⌘9).
    pub fn show_tab_number(&self, number: usize) {
        let tab = {
            let tabs = self.tabs.borrow();
            let paths: Vec<&str> = tabs.paths().collect();
            let at = if number >= 9 { paths.len().checked_sub(1) } else { Some(number - 1) };
            at.and_then(|at| paths.get(at)).map(|path| (*path).to_owned())
        };
        if let Some(path) = tab {
            self.open_note(&path, true);
        }
    }

    /// Closes the tab shown (Cmd+W); false when there is none.
    pub fn close_current_tab(&self) -> bool {
        let Some(path) = self.tabs.borrow().current_path().map(str::to_owned) else { return false };
        self.close_tab(&path);
        true
    }

    /// Closes every tab but `path`'s, which shows.
    pub fn close_other_tabs(&self, path: &str) {
        let Some(i) = self.tabs.borrow().paths().position(|p| p == path) else { return };
        // The tab shown goes unless it is `path`'s: its note is left first.
        let shown_goes = self.tabs.borrow().current_path() != Some(path);
        if shown_goes && !self.leave() {
            return;
        }
        let after = self.tabs.borrow_mut().close_others(i);
        if let AfterClose::Show(next) = after {
            self.open_note(&next, true);
        }
        self.save_tabs();
    }

    /// The document at `path` shown as a tab (ADR 0024): the sidebar's tabs follow. Gives
    /// where the document was when its tab was last left.
    pub(super) fn show_tab(&self, path: &str) -> Option<TabPlace> {
        // A note opens in the tab shown; a PDF, or a note asked to, in a new tab beside it
        // (owner's choice).
        // Beside a deck's review too: a note opened from the list does not take the review's tab.
        let on_review = self.tabs.borrow().current_path().is_some_and(|shown| super::cards::deck_of_tab(shown).is_some());
        let beside = self.new_tab_next.replace(false) || FileKind::from_path(path) == FileKind::Pdf || on_review;
        let place = if beside { self.tabs.borrow_mut().open_beside(path) } else { self.tabs.borrow_mut().navigate(path) };
        self.show_tabs();
        place
    }

    /// The tab shown remembers where its document is, as it is left.
    pub(super) fn remember_place(&self) {
        // Only the current tab's document: a tab just closed gives its place to none.
        let shown = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        if shown.is_none() || shown.as_deref() != self.tabs.borrow().current_path() {
            return;
        }
        let place = if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            Some(TabPlace { top: viewer.place(), cursor: None })
        } else if self.editing.borrow().is_some() {
            let selection = self.note_text.selectedRange();
            self.note_top().map(|top| TabPlace { top: (top, 0.0), cursor: Some((selection.location, selection.length)) })
        } else {
            None
        };
        if let Some(place) = place {
            self.tabs.borrow_mut().remember(place);
        }
    }

    /// The vault's tabs file, beside its index (ADR 0024).
    pub(super) fn tabs_file(&self) -> Option<PathBuf> {
        self.state.borrow().index_dir.as_ref().map(|dir| dir.join(TABS_FILE))
    }

    /// Writes the tabs and their places for the next launch.
    pub(super) fn save_tabs(&self) {
        // Written as the tabs change: shown as they are, too.
        self.show_tabs();
        let Some(file) = self.tabs_file() else { return };
        let text = self.tabs.borrow().to_json().to_string();
        let tmp = file.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &file)) {
            eprintln!("knotes: cannot save tabs: {e}");
        }
    }

    /// The tabs of the last launch, those whose documents are still there; the one shown
    /// then is shown again, where it was. Once, on the vault's first snapshot.
    pub(super) fn restore_tabs(&self) {
        let (Some(file), Some(vault)) = (self.tabs_file(), self.vault()) else { return };
        let Ok(text) = std::fs::read_to_string(file) else { return };
        let mut tabs = Tabs::from_text(&text);
        tabs.retain(|p| super::cards::deck_of_tab(p).is_some() || vault.join(p).is_file());
        let current = tabs.current_path().map(str::to_owned);
        *self.tabs.borrow_mut() = tabs;
        if let Some(path) = current {
            self.open_note(&path, true);
        }
        self.show_tabs();
    }

    /// Opens `path` in a new tab beside the tab shown (the list's menu).
    pub fn open_in_new_tab(&self, path: &str) {
        self.new_tab_next.set(true);
        self.open_note(path, true);
        self.new_tab_next.set(false);
    }

    /// `other`'s tab clicked, then the tab shown before (probe mode).
    pub fn tab_trip(&self, other: &str) {
        let Some(back) = self.tabs.borrow().current_path().map(str::to_owned) else { return };
        self.open_note(other, true);
        self.open_note(&back, true);
    }
}
