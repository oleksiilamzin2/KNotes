//! The sidebar and the note list: what the sidebar shows (a space's part and the rest, ADR 0030) and the notes of what is selected.

use super::*;

impl VaultWindow {
    /// Folders or tags under "All notes" (the toolbar switch); the list keeps what it shows.
    pub fn set_sidebar_mode(&self, mode: SidebarMode) {
        self.state.borrow_mut().sidebar_mode = mode;
        self.toolbar_delegate.set_sidebar_mode(mode);
        self.rebuild_sidebar();
        let selection = self.state.borrow().selection.clone();
        self.highlight(&selection);
    }

    pub(super) fn rebuild_sidebar(&self) {
        // Rebuilding would end the editing of a folder's name; done when it ends.
        if self.sidebar.is_renaming() {
            self.sidebar_waiting.set(true);
            return;
        }
        self.sidebar_waiting.set(false);
        // What it holds is knotes-app's to say (`VaultState::sidebar`).
        let (model, rest_tags) = self.state.borrow().sidebar(strings::NEW_NOTES_FOLDER);
        if let Some(rest_tags) = rest_tags {
            self.state.borrow_mut().rest_tags = rest_tags;
        }
        self.sidebar.set_model(model);
        self.outline.reloadData();
    }

    /// Shows the notes of a sidebar item.
    pub fn select(&self, item: SidebarItem) {
        if self.highlighting.get() {
            return;
        }
        // A deck: its review, in a tab of its own (ADR 0037); the list shows its notes.
        if let SidebarItem::Deck(deck) = &item {
            self.show_deck(deck);
        }
        self.state.borrow_mut().selection = item;
        *self.search_results.borrow_mut() = None;
        self.refresh_list();
        if !self.state.borrow().search.trim().is_empty() {
            self.request_search();
        }
    }

    pub(super) fn refresh_list(&self) {
        // A recently deleted entry shown goes with their list, or when restored or purged.
        let gone = self.previewing.borrow().as_ref().is_some_and(|shown| {
            let state = self.state.borrow();
            state.selection != SidebarItem::Deleted || !state.trash.iter().any(|e| e.id == shown.id)
        });
        if gone {
            self.show_nothing();
        }
        // The recently deleted: their entries, the newest first (ADR 0032).
        if self.state.borrow().selection == SidebarItem::Deleted {
            let (rows, trash) = {
                let state = self.state.borrow();
                (Arc::clone(&state.rows), Arc::clone(&state.trash))
            };
            self.list.set_deleted(Arc::clone(&trash));
            self.list.set_items(rows, (0..trash.len() as u32).map(ListItem::Deleted).collect());
            self.table.reloadData();
            self.window.setTitle(&NSString::from_str(strings::RECENTLY_DELETED));
            self.window.setSubtitle(&NSString::from_str(&strings::entries_count(trash.len())));
            self.toolbar_delegate.set_list_title(strings::RECENTLY_DELETED, &strings::entries_count(trash.len()));
            self.highlight_open_note();
            return;
        }
        let mut state = self.state.borrow_mut();
        let title = match &state.selection {
            SidebarItem::Folder(path) => path.rsplit('/').next().unwrap_or(path).to_owned(),
            SidebarItem::Tag(tag) => format!("#{tag}"),
            SidebarItem::Deck(deck) => deck.rsplit('/').next().unwrap_or(deck).to_owned(),
            SidebarItem::Untagged => strings::UNTAGGED.to_owned(),
            SidebarItem::Rest => strings::REST_OF_VAULT.to_owned(),
            _ => strings::ALL_NOTES.to_owned(),
        };
        // What it holds is knotes-app's to say (`VaultState::listed`): the notes of the row, or
        // a search's finds among them.
        self.ensure_reader(&mut state);
        let among = state.notes_listed();
        let results = self.search_results.borrow();
        let listed = state.listed(among.as_deref(), results.as_ref().map(|r| (r.titles.as_slice(), r.content.as_slice())), &note_list_view::now());
        drop(results);
        self.list_menu.update(&listed.settings, listed.session_key.is_none());
        *self.search_scope.borrow_mut() = listed.scope;
        *self.search_space.borrow_mut() = listed.space_scope;
        self.list.set_items(Arc::clone(&state.rows), listed.items);
        drop(state);
        self.table.reloadData();
        self.highlight_open_note();
        self.window.setTitle(&NSString::from_str(&title));
        let count = self.list.note_count();
        let formatted = NSNumberFormatter::localizedStringFromNumber_numberStyle(
            &NSNumber::numberWithUnsignedLongLong(count as u64),
            NSNumberFormatterStyle::DecimalStyle,
        );
        let notes = strings::notes_count(count, &formatted.to_string());
        self.window.setSubtitle(&NSString::from_str(&notes));
        // Shown in the row under the toolbar (the window's own title is hidden).
        self.toolbar_delegate.set_list_title(&title, &notes);
    }

    /// Selects the open note's row (it moves when the note is saved), or none if it is not listed.
    pub(super) fn highlight_open_note(&self) {
        self.highlighting.set(true);
        let shown = self.previewing.borrow().as_ref().map(|e| e.id.clone());
        let row = match shown {
            Some(id) => self.list.row_of_deleted(&id),
            None => self.open_path().and_then(|p| self.list.row_of(&p)),
        };
        match row {
            Some(row) => self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false),
            // SAFETY: a nil sender is allowed.
            None => unsafe { self.table.deselectAll(None) },
        }
        self.highlighting.set(false);
    }

    pub(super) fn exists(&self, item: &SidebarItem) -> bool {
        self.state.borrow().exists(item, strings::NEW_NOTES_FOLDER)
    }

    /// The path bar's click: the vault's notes (`""`) or a folder's, in the list, its row
    /// selected in the sidebar (its parents opened to show it).
    pub(super) fn show_folder(&self, folder: &str) {
        let item = if folder.is_empty() { SidebarItem::AllNotes } else { SidebarItem::Folder(folder.to_owned()) };
        let mut parent = folder.rsplit_once('/').map(|(p, _)| p);
        let mut parents = Vec::new();
        while let Some(at) = parent {
            parents.push(at.to_owned());
            parent = at.rsplit_once('/').map(|(p, _)| p);
        }
        for at in parents.iter().rev() {
            // SAFETY: an item object of this outline view.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&SidebarItem::Folder(at.clone())))) };
        }
        self.highlight(&item);
        self.select(item);
    }

    /// Selects the row of `item` in the sidebar without notifying (the list follows `state`).
    pub(super) fn highlight(&self, item: &SidebarItem) {
        // SAFETY: the item object comes from this outline view's data source.
        let row = unsafe { self.outline.rowForItem(Some(&self.sidebar.object_for(item))) };
        self.highlighting.set(true);
        if row >= 0 {
            self.outline.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row as usize), false);
        } else {
            // Not shown in this sidebar mode: no other row may look selected.
            // SAFETY: a nil sender is allowed.
            unsafe { self.outline.deselectAll(None) };
        }
        self.highlighting.set(false);
    }
}
