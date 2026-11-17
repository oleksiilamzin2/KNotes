//! The sidebar and the note list: what the sidebar shows (a space's part and the rest, ADR 0030) and the notes of what is selected.

use super::*;

/// The space's tags as the snapshot counted them (ADR 0030), or counted here while the window's
/// folders are ahead of it (a folder just made or put in a space, before the snapshot after).
pub(super) fn counted_space_tags<'a>(state: &'a State, space: &SpaceFolders) -> std::borrow::Cow<'a, SpaceTags> {
    let id = state.space.as_deref().unwrap_or_default();
    match state.space_tags.iter().find(|counted| counted.id == id && counted.folders == *space) {
        Some(counted) => std::borrow::Cow::Borrowed(counted),
        None => std::borrow::Cow::Owned(SpaceTags::count(id, space.clone(), &state.tagged, &state.untagged)),
    }
}

/// Tags and "Untagged" have no folder and no `.knotes.json`: their list options last the session.
pub(super) fn session_settings_key(item: &SidebarItem) -> Option<String> {
    matches!(item, SidebarItem::Tag(_) | SidebarItem::Untagged | SidebarItem::Deck(_)).then(|| item.key())
}

pub(super) fn folder_exists(node: &FolderNode, path: &str) -> bool {
    node.path == path || node.children.iter().any(|c| folder_exists(c, path))
}

/// The folder at `path` in the tree.
pub(super) fn find_folder_node<'a>(node: &'a FolderNode, path: &str) -> Option<&'a FolderNode> {
    if node.path == path {
        return Some(node);
    }
    node.children.iter().find_map(|c| find_folder_node(c, path))
}

/// Every tag of `nodes` and below them.
pub(super) fn tag_names(nodes: &[TagNode]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<&TagNode> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        names.insert(node.tag.clone());
        stack.extend(node.children.iter());
    }
    names
}

pub(super) fn tag_exists(nodes: &[TagNode], tag: &str) -> bool {
    nodes.iter().any(|n| n.tag == tag || tag_exists(&n.children, tag))
}

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
        let (model, rest_tags) = {
            let state = self.state.borrow();
            if state.sidebar_mode == SidebarMode::Cards {
                let model = Sidebar::decks(&state.decks);
                drop(state);
                self.sidebar.set_model(model);
                self.outline.reloadData();
                return;
            }
            if state.sidebar_mode == SidebarMode::Tabs {
                let tabs: Vec<(String, String)> = self.tabs.borrow().paths().map(|p| (p.to_owned(), document_name(p).to_owned())).collect();
                drop(state);
                self.sidebar.set_model(Sidebar::tabs(&tabs));
                self.outline.reloadData();
                return;
            }
            // In a space: its folders and tags (counted there), the inbox if it is one of them,
            // its untagged notes; then the rest of the vault under a row of its own (ADR 0030).
            // Its tags as the snapshot counted them: nothing counted here after every save.
            let space = state.space_folders.as_ref();
            let counted = space.map(|space| counted_space_tags(&state, space));
            let split = space.map(|space| state.folders.split(space));
            let untagged = counted.as_ref().map_or(state.untagged.len(), |counted| counted.untagged);
            let rest_tags = counted.as_ref().map(|counted| tag_names(&counted.rest)).unwrap_or_default();
            let (folders, tags) = match (&split, &counted) {
                (Some((folders, _)), Some(counted)) => (folders, counted.tags.as_slice()),
                _ => (&*state.folders, state.tags.as_slice()),
            };
            let rest = split.as_ref().zip(counted.as_ref()).map(|((_, rest_folders), counted)| RestPart { folders: rest_folders, tags: &counted.rest, notes: rest_folders.notes });
            let inbox = new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER);
            let inbox = state.space_folders.as_ref().is_none_or(|space| space.holds_folder(&inbox)).then_some(inbox);
            let model = Sidebar::new(folders, tags, state.sidebar_mode, inbox.as_deref(), untagged, &|path| resolve_list_settings(path, &state.rules).include_subfolders, rest, state.trash.len());
            (model, rest_tags)
        };
        self.state.borrow_mut().rest_tags = rest_tags;
        self.sidebar.set_model(model);
        self.outline.reloadData();
    }

    /// Shows the notes of a sidebar item.
    pub fn select(&self, item: SidebarItem) {
        if self.highlighting.get() {
            return;
        }
        // A tab: its document shown; the list keeps what it shows.
        if let SidebarItem::Tab(path) = &item {
            self.open_note(path, true);
            return;
        }
        // A deck: its review in the note column; any other list: the review goes.
        match &item {
            SidebarItem::Deck(deck) => self.show_deck(deck),
            _ => self.hide_review(),
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
        let selection = state.selection.clone();
        let (title, listed) = match &selection {
            SidebarItem::Folder(path) => (path.rsplit('/').next().unwrap_or(path).to_owned(), None),
            SidebarItem::Tag(tag) => (format!("#{tag}"), Some(Arc::new(self.notes_with_tag(&mut state, tag)))),
            // A deck: the notes its cards are written in, its sub-decks' too (ADR 0037).
            SidebarItem::Deck(deck) => (deck.rsplit('/').next().unwrap_or(deck).to_owned(), Some(Arc::new(self.notes_with_deck(&mut state, deck)))),
            SidebarItem::Untagged => (strings::UNTAGGED.to_owned(), Some(Arc::clone(&state.untagged))),
            SidebarItem::Rest => (strings::REST_OF_VAULT.to_owned(), None),
            _ => (strings::ALL_NOTES.to_owned(), None),
        };
        let filter = match (&selection, &listed) {
            (SidebarItem::Folder(path), _) => ListFilter::Folder(path),
            (_, Some(paths)) => ListFilter::Paths(paths),
            _ => ListFilter::All,
        };
        let folder = match &selection {
            SidebarItem::Folder(path) => path.as_str(),
            _ => "",
        };
        let session_key = session_settings_key(&selection);
        let settings = match &session_key {
            Some(key) => state.tag_settings.get(key).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules)),
            None => resolve_list_settings(folder, &state.rules),
        };
        self.list_menu.update(&settings, session_key.is_none());
        // Search results replace the list; it is built only without them (50k notes: ~2 ms).
        // What a search here covers: the list's notes and its PDFs (ADR 0022).
        let searching = !state.search.trim().is_empty();
        // In a space: its notes, then the rest of the list's under «Решта бази»; a row of the
        // sidebar's «Решта бази» lists the notes outside the space (ADR 0030).
        let outside = match &selection {
            SidebarItem::Rest => true,
            SidebarItem::Folder(path) => state.space_folders.as_ref().is_some_and(|space| !space.holds_folder(path)),
            SidebarItem::Tag(tag) => state.rest_tags.contains(tag),
            _ => false,
        };
        let space = state.space_folders.as_ref().map(|space| if outside { SpaceList::Rest(space) } else { SpaceList::Space(space) });
        *self.search_scope.borrow_mut() = (searching && !matches!(selection, SidebarItem::AllNotes)).then(|| list_scope(&state.rows, &filter, &settings, space));
        *self.search_space.borrow_mut() = match space {
            Some(SpaceList::Space(folders)) if searching => Some(list_space_scope(&state.rows, &filter, &settings, folders)),
            _ => None,
        };
        let results = self.search_results.borrow();
        let items = match results.as_ref().filter(|_| searching) {
            Some(results) => search_list(&state.rows, &filter, &settings, &results.titles, &results.content, space),
            None => build_note_list(&state.rows, &filter, &settings, &note_list_view::now(), space),
        };
        drop(results);
        self.list.set_items(Arc::clone(&state.rows), items);
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

    pub(super) fn notes_with_tag(&self, state: &mut State, tag: &str) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    pub(super) fn notes_with_deck(&self, state: &mut State, deck: &str) -> HashSet<String> {
        use knotes_core::CardIndex;
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.deck_notes(deck).ok()).unwrap_or_default().into_iter().collect()
    }

    pub(super) fn exists(&self, item: &SidebarItem) -> bool {
        let state = self.state.borrow();
        match item {
            // The inbox row shows before its folder exists.
            // Every folder shows, in a space or under «Решта бази» (ADR 0030); the inbox row
            // before its folder exists, but not outside the space.
            SidebarItem::Folder(path) => {
                let inbox = new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER);
                folder_exists(&state.folders, path) || (*path == inbox && state.space_folders.as_ref().is_none_or(|space| space.holds_folder(&inbox)))
            }
            SidebarItem::Rest => state.space.is_some(),
            SidebarItem::Deleted => !state.trash.is_empty(),
            SidebarItem::Tag(tag) => tag_exists(&state.tags, tag),
            SidebarItem::Deck(deck) => tag_exists(&state.decks, deck),
            _ => true,
        }
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
        // The tabs show the document shown, whatever the list shows.
        let tab = (self.state.borrow().sidebar_mode == SidebarMode::Tabs).then(|| self.tabs.borrow().current_path().map(|p| SidebarItem::Tab(p.to_owned())));
        let item = match &tab {
            Some(Some(tab)) => tab,
            Some(None) => &SidebarItem::Gap,
            None => item,
        };
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
