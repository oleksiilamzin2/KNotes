//! What a vault's window shows, with no window: the last snapshot of the vault, the sidebar
//! row selected, the window's space and the search typed. The window keeps one, fills it from
//! each snapshot and asks it what its sidebar and its list hold.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use knotes_cards::DeckSettings;
use knotes_core::{CardIndex, CardRow, CatalogEntry, DeckCount, DeckStats, DueCard, NoteRow, TagIndex, TrashEntry};
use knotes_search::TitleIndex;
use knotes_spaces::{KeptFromAi, Spaces};

use crate::{
    Clock, ContentMatch, deck_rows, deck_tree, FolderNode, FolderRules, History, ListFilter, ListItem, ListSettings, RestPart, Sidebar, SidebarItem, SidebarMode, SpaceFolders, SpaceList, SpaceTags, TagNode, build_note_list, list_scope, list_space_scope,
    new_notes_folder, resolve_list_settings, search_list,
};

/// What the window shows: the last snapshot and the selected sidebar row. `R` reads the index
/// (opened on first need), `L` resolves links (ADR 0013).
pub struct VaultState<R, L> {
    pub selection: SidebarItem,
    pub rows: Arc<Vec<NoteRow>>,
    pub rules: Arc<HashMap<String, FolderRules>>,
    pub folders: Arc<FolderNode>,
    /// Every folder of the vault, empty ones too (the snapshot's list).
    pub folder_paths: Arc<Vec<String>>,
    pub tags: Arc<Vec<TagNode>>,
    /// The decks of the notes' cards as a tree (ADR 0037), each with its cards to review.
    pub decks: Arc<Vec<TagNode>>,
    /// Notes without any tag (the "Untagged" row), and every tag with its note, a note's
    /// together (a space's tags count from them, ADR 0030): the last snapshot's.
    pub untagged: Arc<HashSet<String>>,
    pub tagged: Arc<Vec<(String, String)>>,
    /// Each space's tags and untagged notes, as the last snapshot counted them.
    pub space_tags: Arc<Vec<SpaceTags>>,
    pub index_dir: Option<PathBuf>,
    pub vault: Option<PathBuf>,
    /// List settings of tag views: tags have no folder and no `.knotes.json`; kept for the session.
    pub tag_settings: HashMap<String, ListSettings>,
    pub history: History,
    /// Read-only index connection, opened on first need (the index exists after the first snapshot).
    pub reader: Option<R>,
    /// Link resolution of the last snapshot (ADR 0013); `None` until the first one.
    pub links: Option<Arc<L>>,
    /// Every vault file of the last snapshot, to resolve links as they will be after a move.
    pub catalog: Arc<Vec<CatalogEntry>>,
    /// Title search of the last snapshot, for link completion.
    pub titles: Option<Arc<TitleIndex>>,
    /// Folders or tags under "All notes".
    pub sidebar_mode: SidebarMode,
    /// Text in the search field over the list; the list shows its results when not empty.
    pub search: String,
    /// The vault's spaces (ADR 0030), the window's (none: the whole vault) and its folders.
    pub spaces: Arc<Spaces>,
    pub space: Option<String>,
    pub space_folders: Option<SpaceFolders>,
    /// The tags the sidebar lists under «Решта бази» (none there in the space).
    pub rest_tags: HashSet<String>,
    /// The recently deleted, the newest first (ADR 0032).
    pub trash: Arc<Vec<TrashEntry>>,
    /// What is kept from the AI (ADR 0036).
    pub ai: Arc<KeptFromAi>,
}

impl<R, L> Default for VaultState<R, L> {
    /// Before the first snapshot: all notes selected, nothing in them.
    fn default() -> Self {
        Self {
            selection: SidebarItem::AllNotes,
            rows: Arc::default(),
            rules: Arc::default(),
            folders: Arc::default(),
            folder_paths: Arc::default(),
            tags: Arc::default(),
            decks: Arc::default(),
            untagged: Arc::default(),
            tagged: Arc::default(),
            space_tags: Arc::default(),
            index_dir: None,
            vault: None,
            tag_settings: HashMap::new(),
            history: History::default(),
            reader: None,
            links: None,
            catalog: Arc::default(),
            titles: None,
            sidebar_mode: SidebarMode::Folders,
            search: String::new(),
            spaces: Arc::default(),
            space: None,
            space_folders: None,
            rest_tags: HashSet::new(),
            trash: Arc::default(),
            ai: Arc::default(),
        }
    }
}

/// What the note list shows, and what a search typed over it covers.
pub struct Listed {
    pub items: Vec<ListItem>,
    /// How the list is sorted and grouped, and the key these settings are kept under for the
    /// session when the row has no folder to keep them in (a tag, a deck, «Без тегів»).
    pub settings: ListSettings,
    pub session_key: Option<String>,
    /// While a search is typed: the list's notes and PDFs it covers (none: the whole vault),
    /// and in a space the part of them in it, found apart (ADR 0030).
    pub scope: Option<Vec<String>>,
    pub space_scope: Option<Vec<String>>,
}

/// Tags, decks and "Untagged" have no folder and no `.knotes.json`: their list options last
/// the session, under this key.
pub fn session_settings_key(item: &SidebarItem) -> Option<String> {
    matches!(item, SidebarItem::Tag(_) | SidebarItem::Untagged | SidebarItem::Deck(_)).then(|| item.key())
}

pub fn folder_exists(node: &FolderNode, path: &str) -> bool {
    node.path == path || node.children.iter().any(|c| folder_exists(c, path))
}

/// The folder at `path` in the tree.
pub fn find_folder_node<'a>(node: &'a FolderNode, path: &str) -> Option<&'a FolderNode> {
    if node.path == path {
        return Some(node);
    }
    node.children.iter().find_map(|c| find_folder_node(c, path))
}

/// Every tag of `nodes` and below them.
pub fn tag_names(nodes: &[TagNode]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<&TagNode> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        names.insert(node.tag.clone());
        stack.extend(node.children.iter());
    }
    names
}

pub fn tag_exists(nodes: &[TagNode], tag: &str) -> bool {
    nodes.iter().any(|n| n.tag == tag || tag_exists(&n.children, tag))
}

/// Whether the decks have `deck`; the deck `""`, all of them together, while there is any.
pub fn deck_exists(decks: &[TagNode], deck: &str) -> bool {
    if deck.is_empty() { !decks.is_empty() } else { tag_exists(decks, deck) }
}

impl<R, L> VaultState<R, L> {
    /// The window's space as the vault has it now: none if it is gone, and its folders as the
    /// tree has them.
    pub fn fit_space(&mut self) {
        if self.space.as_ref().is_some_and(|id| self.spaces.get(id).is_none()) {
            self.space = None;
        }
        self.space_folders = self.space.as_ref().map(|id| SpaceFolders::new(&self.folders, &self.spaces, id));
    }

    /// The space's tags as the snapshot counted them (ADR 0030), or counted here while the
    /// window's folders are ahead of it (a folder just made or put in a space, before the
    /// snapshot after).
    pub fn counted_space_tags(&self, space: &SpaceFolders) -> Cow<'_, SpaceTags> {
        let id = self.space.as_deref().unwrap_or_default();
        match self.space_tags.iter().find(|counted| counted.id == id && counted.folders == *space) {
            Some(counted) => Cow::Borrowed(counted),
            None => Cow::Owned(SpaceTags::count(id, space.clone(), &self.tagged, &self.untagged)),
        }
    }

    /// The sidebar's rows, and the tags it lists under «Решта бази» (none said for the decks,
    /// which have no such row: what was there stays). `inbox_name`: the folder of new notes
    /// when no rule names one; `all_decks`: the name of the row for every deck together.
    pub fn sidebar(&self, inbox_name: &str, all_decks: &str) -> (Sidebar, Option<HashSet<String>>) {
        if self.sidebar_mode == SidebarMode::Cards {
            return (Sidebar::decks(&self.decks, all_decks), None);
        }
        // In a space: its folders and tags (counted there), the inbox if it is one of them,
        // its untagged notes; then the rest of the vault under a row of its own (ADR 0030).
        // Its tags as the snapshot counted them: nothing counted here after every save.
        let space = self.space_folders.as_ref();
        let counted = space.map(|space| self.counted_space_tags(space));
        let split = space.map(|space| self.folders.split(space));
        let untagged = counted.as_ref().map_or(self.untagged.len(), |counted| counted.untagged);
        let rest_tags = counted.as_ref().map(|counted| tag_names(&counted.rest)).unwrap_or_default();
        let (folders, tags) = match (&split, &counted) {
            (Some((folders, _)), Some(counted)) => (folders, counted.tags.as_slice()),
            _ => (&*self.folders, self.tags.as_slice()),
        };
        let rest = split.as_ref().zip(counted.as_ref()).map(|((_, rest_folders), counted)| RestPart { folders: rest_folders, tags: &counted.rest, notes: rest_folders.notes });
        let inbox = new_notes_folder(&self.rules, inbox_name);
        let inbox = self.space_folders.as_ref().is_none_or(|space| space.holds_folder(&inbox)).then_some(inbox);
        let model = Sidebar::new(folders, tags, self.sidebar_mode, inbox.as_deref(), untagged, &|path| resolve_list_settings(path, &self.rules).include_subfolders, rest, self.trash.len());
        (model, Some(rest_tags))
    }

    /// Whether the sidebar has a row for `item`.
    pub fn exists(&self, item: &SidebarItem, inbox_name: &str) -> bool {
        match item {
            // Every folder shows, in a space or under «Решта бази» (ADR 0030); the inbox row
            // before its folder exists, but not outside the space.
            SidebarItem::Folder(path) => {
                let inbox = new_notes_folder(&self.rules, inbox_name);
                folder_exists(&self.folders, path) || (*path == inbox && self.space_folders.as_ref().is_none_or(|space| space.holds_folder(&inbox)))
            }
            SidebarItem::Rest => self.space.is_some(),
            SidebarItem::Deleted => !self.trash.is_empty(),
            SidebarItem::Tag(tag) => tag_exists(&self.tags, tag),
            SidebarItem::Deck(deck) => deck_exists(&self.decks, deck),
            _ => true,
        }
    }

    /// The row selected, all notes instead when the sidebar has it no more (its folder, tag or
    /// deck gone, the space left, the last deleted entry restored).
    pub fn fit_selection(&mut self, inbox_name: &str) -> SidebarItem {
        if !self.exists(&self.selection, inbox_name) {
            self.selection = SidebarItem::AllNotes;
        }
        self.selection.clone()
    }

    /// The note list of the row selected (not of the recently deleted, which are no notes).
    /// `among`: the notes a tag, a deck or «Без тегів» lists (`notes_listed`); `found`: a
    /// search's answer (the names, the text), which replaces the list while a search is typed.
    pub fn listed(&self, among: Option<&HashSet<String>>, found: Option<(&[String], &[ContentMatch])>, clock: &Clock) -> Listed {
        let selection = &self.selection;
        let filter = match (selection, among) {
            (SidebarItem::Folder(path), _) => ListFilter::Folder(path),
            (_, Some(paths)) => ListFilter::Paths(paths),
            _ => ListFilter::All,
        };
        let folder = match selection {
            SidebarItem::Folder(path) => path.as_str(),
            _ => "",
        };
        let session_key = session_settings_key(selection);
        let settings = match &session_key {
            Some(key) => self.tag_settings.get(key).copied().unwrap_or_else(|| resolve_list_settings("", &self.rules)),
            None => resolve_list_settings(folder, &self.rules),
        };
        // Search results replace the list; it is built only without them (50k notes: ~2 ms).
        // What a search here covers: the list's notes and its PDFs (ADR 0022).
        let searching = !self.search.trim().is_empty();
        // In a space: its notes, then the rest of the list's under «Решта бази»; a row of the
        // sidebar's «Решта бази» lists the notes outside the space (ADR 0030).
        let outside = match selection {
            SidebarItem::Rest => true,
            SidebarItem::Folder(path) => self.space_folders.as_ref().is_some_and(|space| !space.holds_folder(path)),
            SidebarItem::Tag(tag) => self.rest_tags.contains(tag),
            _ => false,
        };
        let space = self.space_folders.as_ref().map(|space| if outside { SpaceList::Rest(space) } else { SpaceList::Space(space) });
        let scope = (searching && !matches!(selection, SidebarItem::AllNotes)).then(|| list_scope(&self.rows, &filter, &settings, space));
        let space_scope = match space {
            Some(SpaceList::Space(folders)) if searching => Some(list_space_scope(&self.rows, &filter, &settings, folders)),
            _ => None,
        };
        let items = match found.filter(|_| searching) {
            Some((titles, content)) => search_list(&self.rows, &filter, &settings, titles, content, space),
            None => build_note_list(&self.rows, &filter, &settings, clock, space),
        };
        Listed { items, settings, session_key, scope, space_scope }
    }
}

impl<R: TagIndex + CardIndex, L> VaultState<R, L> {
    /// The notes with `tag`, or one under it (none before the reader is open).
    pub fn notes_with_tag(&self, tag: &str) -> HashSet<String> {
        self.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    /// The notes `deck`'s cards are written in, its sub-decks' too (ADR 0037). In a space, its
    /// notes only: the deck is the space's part of it.
    pub fn notes_with_deck(&self, deck: &str) -> HashSet<String> {
        let notes = self.reader.as_ref().and_then(|r| r.deck_notes(deck).ok()).unwrap_or_default();
        notes.into_iter().filter(|path| self.space.as_ref().is_none_or(|id| self.spaces.holds(path, id))).collect()
    }

    /// The notes the row selected lists by name: a tag's, a deck's, those with no tag. None for
    /// a row that lists by place (a folder, all notes, the rest of the vault).
    pub fn notes_listed(&self) -> Option<Arc<HashSet<String>>> {
        match &self.selection {
            SidebarItem::Tag(tag) => Some(Arc::new(self.notes_with_tag(tag))),
            SidebarItem::Deck(deck) => Some(Arc::new(self.notes_with_deck(deck))),
            SidebarItem::Untagged => Some(Arc::clone(&self.untagged)),
            _ => None,
        }
    }
}

/// The decks and their cards (ADR 0037). In a space (ADR 0030) the decks are those of its
/// notes' cards, counted and reviewed from them (owner's request: decks follow the spaces by
/// their files); for the whole vault the index counts itself. Times are seconds since 1970:
/// `now`, and `day_start`, when today began (a day's new cards count from it).
impl<R: CardIndex, L> VaultState<R, L> {
    /// In a space, the cards written in its notes; none for the whole vault.
    fn space_rows(&self) -> Option<Vec<CardRow>> {
        let id = self.space.as_ref()?;
        let mut rows = self.reader.as_ref().and_then(|r| r.card_rows(None, None).ok()).unwrap_or_default();
        rows.retain(|row| self.spaces.holds(&row.path, id));
        Some(rows)
    }

    /// Every card of `deck` and the decks under it (of all the decks, for `""`), by note and in
    /// the note's order; in a space, those of its notes. With where each stands: a deck's table.
    pub fn card_rows(&self, deck: &str) -> Vec<CardRow> {
        let mut rows = self.reader.as_ref().and_then(|r| r.card_rows((!deck.is_empty()).then_some(deck), None).ok()).unwrap_or_default();
        if let Some(id) = &self.space {
            rows.retain(|row| self.spaces.holds(&row.path, id));
        }
        rows
    }

    /// The cards `deck` asks now, `limit` at most: those due, and of the new as many as it
    /// still asks today by its settings (what its settings allow a day, less those first
    /// reviewed today in it and under it). Asked of the index, not as the last snapshot had it:
    /// a review just done counts.
    pub fn due_cards(&self, deck: &str, settings: &DeckSettings, now: i64, day_start: i64, limit: usize) -> Vec<DueCard> {
        // All the decks together (`""`): what each of the top ones asks, by its own settings (a
        // day's new cards are a deck's, not shared out among them): those due first, the
        // longest due first, then the new ones, deck by deck.
        if deck.is_empty() {
            let mut all: Vec<DueCard> = self.decks.iter().flat_map(|top| self.due_cards(&top.tag, settings, now, day_start, limit)).collect();
            all.sort_by_key(|card| (card.due.is_none(), card.due));
            all.truncate(limit);
            return all;
        }
        let Some(reader) = self.reader.as_ref() else { return Vec::new() };
        let rows = self.space_rows();
        let introduced = match &rows {
            Some(rows) => deck_rows::introduced(rows, day_start),
            None => reader.introduced(day_start).unwrap_or_default(),
        };
        let under = format!("{deck}/");
        let today: usize = introduced.iter().filter(|(d, _)| d == deck || d.starts_with(&under)).map(|(_, n)| n).sum();
        let new = (settings.new_a_day(deck) as usize).saturating_sub(today);
        match &rows {
            Some(rows) => deck_rows::due_cards(rows, deck, now, limit, new),
            None => reader.due_cards(Some(deck), now, limit, new).unwrap_or_default(),
        }
    }

    /// `deck`'s reviewed cards not due yet, the soonest due first, `limit` at most: what a
    /// review ahead of the schedule asks.
    pub fn cards_ahead(&self, deck: &str, now: i64, limit: usize) -> Vec<DueCard> {
        match self.space_rows() {
            Some(rows) => deck_rows::cards_ahead(&rows, deck, now, limit),
            None => self.reader.as_ref().and_then(|r| r.cards_ahead(deck, now, limit).ok()).unwrap_or_default(),
        }
    }

    /// The decks' tree with what each asks now: those due, and of the new as many as it still
    /// asks today. `counts`: the vault's decks as the last snapshot counted them (in a space,
    /// its notes' cards are counted here instead).
    pub fn deck_tree(&self, counts: &[DeckCount], settings: &DeckSettings, now: i64, day_start: i64) -> Vec<TagNode> {
        let new_a_day = |deck: &str| settings.new_a_day(deck) as usize;
        match self.space_rows() {
            Some(rows) => deck_tree(&deck_rows::deck_counts(&rows, now), &deck_rows::introduced(&rows, day_start), &new_a_day),
            None => deck_tree(counts, &self.reader.as_ref().and_then(|r| r.introduced(day_start).ok()).unwrap_or_default(), &new_a_day),
        }
    }

    /// How `deck` stands, with those under it; in a space, of its notes' cards. None for a deck
    /// with no card there.
    pub fn deck_stats(&self, deck: &str, day_start: i64) -> Option<DeckStats> {
        let holds = |path: &str| self.space.as_ref().is_none_or(|id| self.spaces.holds(path, id));
        self.reader.as_ref().and_then(|r| r.deck_stats(deck, day_start, &holds).ok()).filter(|stats| stats.cards > 0)
    }
}
