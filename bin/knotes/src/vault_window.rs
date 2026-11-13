//! A window showing one vault: owns the AppKit objects and applies session events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};

use knotes_app::{Moves, Relinks, embed_images, pasted_image_name, plan_relinks, 
    FolderNode, FolderRules, History, ListFilter, ListOption, ListSettings, SearchMode, Sidebar, SidebarItem, SidebarMode, TagNode, parse_search,
    search_list, with_search_mode, build_note_list, new_notes_folder,
    note_file_stem, resolve_list_settings, unique_note_path,
};
use knotes_core::{AttachmentStore, CatalogEntry, FileKind, FolderRulesStore, Link, LinkKind, LinkResolver, NoteMover, NoteParser, NoteRow, NoteWriter, TagIndex, VaultReader};
use knotes_md::MarkdownParser;
use knotes_core::TitleSearch;
use knotes_editor::{DiskChange, Document, LinkCompletion, conflict_path, link_completion, note_headings, subpath_offset, utf16_offsets};
use knotes_index::{LinkTargets, SqliteIndex};
use knotes_search::TitleIndex;
use knotes_session::{Event, Snapshot};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::sel;
use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSModalResponse, NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSTextInputClient, NSViewController, NSWindow, NSWorkspace};
use objc2_foundation::{NSDate, NSDateFormatter, NSDictionary, NSIndexSet, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSRange, NSString, NSURL};

use crate::chat_view::{self, ChatView};
use crate::collapse_observer::CollapseObserver;
use crate::completion_panel::{CompletionItem, CompletionPanel, MAX_ROWS};
use crate::list_menu::ListMenu;
use crate::live_preview::LivePreview;
use crate::rename_dialog::{self, LinkUpdate};
use crate::note_list_view::{self, NoteListSource};
use crate::note_view::{self, NoteTextView};
use crate::find_bar::FindBar;
use crate::search_bar::SearchBar;
use crate::search_worker::{SearchRequest, SearchResponse, SearchWorker};
use crate::sidebar_view::{self, SidebarSource};
use crate::toolbar::{self, ToolbarDelegate};
use crate::{probe, strings, window};

/// What the window shows: the last snapshot and the selected sidebar row.
struct State {
    selection: SidebarItem,
    rows: Arc<Vec<NoteRow>>,
    rules: Arc<HashMap<String, FolderRules>>,
    folders: Arc<FolderNode>,
    tags: Arc<Vec<TagNode>>,
    /// Notes without any tag (the "Untagged" row).
    untagged: usize,
    index_dir: Option<PathBuf>,
    vault: Option<PathBuf>,
    /// List settings of tag views: tags have no folder and no `.knotes.json`; kept for the session.
    tag_settings: HashMap<String, ListSettings>,
    history: History,
    /// Read-only index connection, opened on first need (the index exists after the first snapshot).
    reader: Option<SqliteIndex>,
    /// Link resolution of the last snapshot (ADR 0013); `None` until the first one.
    links: Option<Arc<LinkTargets>>,
    /// Every vault file of the last snapshot, to resolve links as they will be after a move.
    catalog: Arc<Vec<CatalogEntry>>,
    /// Title search of the last snapshot, for link completion.
    titles: Option<Arc<TitleIndex>>,
    /// Folders or tags under "All notes".
    sidebar_mode: SidebarMode,
    /// Text in the search field over the list; the list shows its results when not empty.
    search: String,
}

/// Autosave runs after this long without edits (ADR 0011).
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

/// The note open in the editor.
struct Editing {
    path: String,
    document: Document,
}

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
    pub list_menu: ListMenu,
    pub note_text: Retained<NoteTextView>,
    /// The search row over the note list.
    pub search_bar: SearchBar,
    /// Find in the open note (Cmd+F), over the note.
    pub find_bar: FindBar,
    /// The occurrences found in the note (UTF-16) and the one selected.
    find_matches: RefCell<Vec<(usize, usize)>>,
    find_current: Cell<Option<usize>>,
    /// Delegate of `note_text`, kept alive here.
    live_preview: Retained<LivePreview>,
    pub chat: ChatView,
    /// Kept alive here: the toolbar holds its delegate weakly.
    toolbar_delegate: Retained<ToolbarDelegate>,
    state: RefCell<State>,
    /// Set while the window selects a sidebar or list row itself, so selection callbacks ignore it.
    highlighting: Cell<bool>,
    editing: RefCell<Option<Editing>>,
    /// Bumped on every edit: a scheduled autosave runs only if no edit came after it.
    edits: Cell<u64>,
    this: RefCell<Weak<Self>>,
    completion: CompletionPanel,
    /// UTF-16 start of the text an accepted completion replaces.
    completion_from: Cell<usize>,
    /// The `[[` (UTF-16 location) whose list Esc closed: it stays closed while typing on.
    completion_dismissed: Cell<Option<usize>>,
    /// Set while a completion is inserted, so the edit does not reopen the list.
    accepting: Cell<bool>,
    /// List search: the background thread, the latest request and the results shown.
    search_worker: RefCell<Option<SearchWorker>>,
    search_generation: Cell<u64>,
    search_results: RefCell<Option<SearchResponse>>,
    /// The sidebar's split view item, watched to hide the folders/tags switch with it.
    sidebar_item: Retained<NSSplitViewItem>,
    collapse_observer: RefCell<Option<Retained<CollapseObserver>>>,
}

impl Drop for VaultWindow {
    fn drop(&mut self) {
        if let Some(observer) = self.collapse_observer.take() {
            observer.stop(&self.sidebar_item);
        }
    }
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Rc<Self> {
        let empty = Sidebar::new(&Default::default(), &[], SidebarMode::Folders, None, 0, &|_| false);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
        let list = NoteListSource::new(mtm);
        let (list_scroll, table) = note_list_view::table(mtm, &list);
        let list_vc = NSViewController::new(mtm);
        let search_bar = SearchBar::new(mtm, list_scroll);
        list_vc.setView(&search_bar.column);
        let (note_scroll, note_text) = note_view::note_view(mtm);
        let live_preview = LivePreview::new(mtm);
        note_text.setDelegate(Some(ProtocolObject::from_ref(&*live_preview)));
        live_preview.attach(&note_text);
        let clicks = live_preview.clone();
        note_text.on_click(move |view, index, cmd| clicks.click(view, index, cmd));
        let hovers = live_preview.clone();
        note_text.on_hover(move |index| hovers.clickable(index));
        let resizes = live_preview.clone();
        note_text.on_resize(move |view, line, width, done| resizes.resize_image(view, line, width, done));
        let files = live_preview.clone();
        note_text.on_image_file(move |view, line| files.image_file(view, line));
        // Weak: the text view keeps its callbacks, and the text view is the live preview's.
        let preview = live_preview.clone();
        let text = objc2::rc::Weak::new(&*note_text);
        note_text.on_appearance(move || {
            if let Some(text) = text.load() {
                preview.appearance_changed(&text);
            }
        });
        let note_vc = NSViewController::new(mtm);
        let find_bar = FindBar::new(mtm, note_scroll.clone());
        note_vc.setView(&find_bar.column);
        let chat = chat_view::chat_view(mtm);
        let chat_vc = NSViewController::new(mtm);
        chat_vc.setView(&chat.view);
        let (window, split) = window::build(mtm, &sidebar_vc, &list_vc, &note_vc, &chat_vc);
        let list_menu = ListMenu::new(mtm);
        let toolbar = ToolbarDelegate::new(mtm, split.splitView(), list_menu.menu.clone());
        search_bar.set_target(toolbar.as_ref());
        toolbar::install(&window, &toolbar);
        let state = State {
            selection: SidebarItem::AllNotes,
            rows: Arc::new(Vec::new()),
            rules: Arc::new(HashMap::new()),
            folders: Arc::new(FolderNode::default()),
            tags: Arc::new(Vec::new()),
            untagged: 0,
            index_dir: None,
            vault: None,
            tag_settings: HashMap::new(),
            history: History::default(),
            reader: None,
            links: None,
            catalog: Arc::new(Vec::new()),
            titles: None,
            sidebar_mode: SidebarMode::Folders,
            search: String::new(),
        };
        let this = Rc::new(Self {
            window,
            sidebar,
            outline,
            list,
            table,
            list_menu,
            note_text,
            search_bar,
            find_bar,
            find_matches: RefCell::new(Vec::new()),
            find_current: Cell::new(None),
            live_preview,
            chat,
            toolbar_delegate: toolbar,
            state: RefCell::new(state),
            highlighting: Cell::new(false),
            editing: RefCell::new(None),
            edits: Cell::new(0),
            this: RefCell::new(Weak::new()),
            completion: CompletionPanel::new(mtm),
            completion_from: Cell::new(0),
            completion_dismissed: Cell::new(None),
            accepting: Cell::new(false),
            search_worker: RefCell::new(None),
            search_generation: Cell::new(0),
            search_results: RefCell::new(None),
            sidebar_item: split.splitViewItems().objectAtIndex(0),
            collapse_observer: RefCell::new(None),
        });
        let toolbar = this.toolbar_delegate.clone();
        let list_item = split.splitViewItems().objectAtIndex(1);
        let split_view = split.splitView();
        let list_width = Cell::new(None::<f64>);
        let observer = CollapseObserver::watch(mtm, &this.sidebar_item, move |collapsed| {
            toolbar.set_sidebar_shown(!collapsed);
            let list = list_item.viewController(mtm).view();
            // With the sidebar collapsed, the traffic lights, the sidebar button, the title and
            // the list options all sit over the list column: it must be wide enough for the
            // toolbar separator to stay on the list/note boundary. The list's own width comes
            // back with the sidebar.
            if collapsed {
                list_width.set(Some(list.frame().size.width));
                list_item.setMinimumThickness(COLLAPSED_LIST_MIN);
            } else {
                list_item.setMinimumThickness(COLUMNS[1].0);
                if let Some(width) = list_width.take() {
                    let (list, split_view) = (MainThreadBound::new(list, mtm), MainThreadBound::new(split_view.clone(), mtm));
                    // Once the sidebar is back in place.
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                        let list = list.get(mtm);
                        split_view.get(mtm).setPosition_ofDividerAtIndex(list.frame().origin.x + width, 1);
                    });
                }
            }
        });
        *this.collapse_observer.borrow_mut() = Some(observer);
        *this.this.borrow_mut() = Rc::downgrade(&this);
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_link(move |link| {
            if let Some(window) = weak.upgrade() {
                window.follow_link(link);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_change(move || {
            if let Some(window) = weak.upgrade() {
                window.edited();
                window.update_completion();
                // An edit drops the highlights: the occurrences are found again, in place.
                if window.find_bar.is_shown() {
                    window.find_text(&window.find_bar.text(), false);
                }
            }
        });
        let (text_weak, step_weak, close_weak) = (Rc::downgrade(&this), Rc::downgrade(&this), Rc::downgrade(&this));
        this.find_bar.on_events(
            move |text| {
                if let Some(window) = text_weak.upgrade() {
                    window.find_text(&text, true);
                }
            },
            move |forward| {
                if let Some(window) = step_weak.upgrade() {
                    window.find_step(forward);
                }
            },
            move || {
                if let Some(window) = close_weak.upgrade() {
                    window.close_find();
                }
            },
        );
        let weak: Weak<Self> = Rc::downgrade(&this);
        let pastes: Weak<Self> = Rc::downgrade(&this);
        this.note_text.on_paste(move |pasteboard, at| pastes.upgrade().is_some_and(|window| window.paste_images(pasteboard, at)));
        this.note_text.on_find(move |action| {
            if let Some(window) = weak.upgrade() {
                window.find_action(action);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_cursor(move || {
            if let Some(window) = weak.upgrade() {
                window.update_completion();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_command(move |command| weak.upgrade().is_some_and(|window| window.completion_command(command)));
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_select(move |item| {
            if let Some(window) = weak.upgrade() {
                window.select(item);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.completion.on_click(move |row| {
            if let Some(window) = weak.upgrade() {
                window.completion.select(row);
                window.accept_completion();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_rename(move |path, name| {
            if let Some(window) = weak.upgrade() {
                window.rename_note(&path, &name);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_move(move |path| {
            if let Some(window) = weak.upgrade() {
                window.move_note_to_chosen_folder(&path);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_rename(move |folder, name| {
            if let Some(window) = weak.upgrade() {
                window.rename_folder(&folder, &name);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_drop(move |path, folder| {
            if let Some(window) = weak.upgrade() {
                window.move_note_to(&path, &folder);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_select(move |path| {
            if let Some(window) = weak.upgrade()
                && !window.highlighting.get()
                && window.open_path().as_deref() != Some(path.as_str())
            {
                window.open_note(&path, true);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_new_note(move || {
            if let Some(window) = weak.upgrade() {
                window.new_note();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_toggle_search(move || {
            if let Some(window) = weak.upgrade() {
                window.toggle_search();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_search_mode(move |mode| {
            if let Some(window) = weak.upgrade() {
                window.set_search_mode(mode);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_search(move |query| {
            if let Some(window) = weak.upgrade() {
                window.search(&query);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        let weak_title: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_note_title(move |name| {
            if let Some(window) = weak_title.upgrade()
                && let Some(open) = window.open_path()
                && !window.rename_note(&open, &name)
            {
                // Declined or impossible: the toolbar shows the name the note kept.
                window.update_navigation();
            }
        });
        this.toolbar_delegate.on_navigate(move |forward| {
            if let Some(window) = weak.upgrade() {
                window.navigate(forward);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list_menu.on_choose(move |option| {
            if let Some(window) = weak.upgrade() {
                window.choose(option);
            }
        });
        this
    }

    /// Folders or tags under "All notes" (the toolbar switch); the list keeps what it shows.
    pub fn set_sidebar_mode(&self, mode: SidebarMode) {
        self.state.borrow_mut().sidebar_mode = mode;
        self.toolbar_delegate.set_sidebar_mode(mode);
        self.rebuild_sidebar();
        let selection = self.state.borrow().selection.clone();
        self.highlight(&selection);
    }

    /// Called when the user flips the folders/tags switch (the app saves the choice).
    pub fn on_sidebar_mode(&self, callback: impl Fn(SidebarMode) + 'static) {
        let weak = self.this.borrow().clone();
        self.toolbar_delegate.on_sidebar_mode(move |mode| {
            if let Some(window) = weak.upgrade() {
                window.set_sidebar_mode(mode);
            }
            callback(mode);
        });
    }

    fn rebuild_sidebar(&self) {
        let model = {
            let state = self.state.borrow();
            let inbox = new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER);
            Sidebar::new(&state.folders, &state.tags, state.sidebar_mode, Some(&inbox), state.untagged, &|path| {
                resolve_list_settings(path, &state.rules).include_subfolders
            })
        };
        self.sidebar.set_model(model);
        self.outline.reloadData();
    }

    /// Images pasted (Cmd+V) or dropped (at character `at`) into the open note: copied into
    /// the attachments folder (one already in the vault stays where it is) and embedded where
    /// the cursor is or the drop falls, each on a line of its own (ADR 0020). False when the
    /// pasteboard brings no image: the text view pastes as usual.
    pub fn paste_images(&self, pasteboard: &NSPasteboard, at: Option<usize>) -> bool {
        let Some(vault) = self.state.borrow().vault.clone() else { return false };
        let found = pasteboard_images(pasteboard, &vault);
        if found.is_empty() {
            return false;
        }
        let store = FsVault::new(&vault);
        let paths: Vec<String> = found
            .into_iter()
            .filter_map(|image| match image {
                PastedImage::InVault(path) => Some(path),
                PastedImage::New { name, bytes } => store.add_attachment(&name, &bytes).inspect_err(|e| eprintln!("knotes: cannot add {name}: {e}")).ok(),
            })
            .collect();
        if paths.is_empty() {
            return true;
        }
        let view: &objc2_app_kit::NSTextView = &self.note_text;
        let text = view.string();
        if let Some(at) = at {
            view.setSelectedRange(NSRange::new(at.min(text.length()), 0));
        }
        let selection = view.selectedRange();
        let end = selection.location + selection.length;
        let line_start = text.lineRangeForRange(NSRange::new(selection.location, 0)).location;
        let after_line = text.lineRangeForRange(NSRange::new(end, 0));
        let line_end = (after_line.location + after_line.length).max(end);
        let before = text.substringWithRange(NSRange::new(line_start, selection.location - line_start)).to_string();
        let after = text.substringWithRange(NSRange::new(end, line_end - end)).to_string();
        let catalog = Arc::clone(&self.state.borrow().catalog);
        let bare = |path: &str| {
            let name = path.rsplit('/').next().unwrap_or(path);
            !catalog.iter().any(|e| e.path != path && e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        };
        let embeds = embed_images(&paths, &before, after.trim_end_matches(['\n', '\r']), bare);
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&embeds), not_found) };
        true
    }

    pub fn set_vault(&self, vault: PathBuf, index_dir: PathBuf) {
        self.live_preview.set_vault(vault.clone());
        let mut state = self.state.borrow_mut();
        state.vault = Some(vault);
        state.index_dir = Some(index_dir);
    }

    /// Applies a list options choice: saved in the folder's `.knotes.json` (only that field), or
    /// kept for the session for tag views.
    pub fn choose(&self, option: ListOption) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        match session_settings_key(&selection) {
            Some(key) => {
                let current = state.tag_settings.get(&key).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules));
                state.tag_settings.insert(key, current.with(option));
            }
            None => {
                let folder = match &selection {
                    SidebarItem::Folder(path) => path.clone(),
                    _ => String::new(),
                };
                let mut rules = state.rules.get(&folder).cloned().unwrap_or_else(|| FolderRules::parse("{}").expect("empty object"));
                rules.set_option(option);
                if let Some(vault) = &state.vault
                    && let Err(e) = FsVault::new(vault).write_rules(&folder, &rules.to_json())
                {
                    eprintln!("knotes: cannot save {folder}/.knotes.json: {e}");
                }
                Arc::make_mut(&mut state.rules).insert(folder, rules);
            }
        }
        drop(state);
        self.refresh_list();
    }

    pub fn apply(&self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => self.show(&snapshot),
            Event::Error(e) => eprintln!("knotes: indexing stopped: {e}"),
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        {
            let open = self.open_path();
            let mut state = self.state.borrow_mut();
            let paths: HashSet<&str> = snapshot.rows.iter().map(|r| r.path.as_str()).collect();
            // The open note stays even when this snapshot predates its file (just created or
            // renamed).
            state.history.retain(|p| paths.contains(p) || open.as_deref() == Some(p));
        }
        let changed = self.open_path().filter(|open| snapshot.changed.iter().any(|p| p == open));
        {
            let mut state = self.state.borrow_mut();
            state.rows = Arc::clone(&snapshot.rows);
            state.rules = Arc::clone(&snapshot.rules);
            state.folders = Arc::clone(&snapshot.folders);
            state.tags = Arc::clone(&snapshot.tags);
            state.untagged = snapshot.untagged;
            state.catalog = Arc::clone(&snapshot.catalog);
        }
        let new_links = {
            let mut state = self.state.borrow_mut();
            let fresh = !state.links.as_ref().is_some_and(|l| Arc::ptr_eq(l, &snapshot.links));
            state.titles = Some(Arc::clone(&snapshot.titles));
            fresh.then(|| {
                state.links = Some(Arc::clone(&snapshot.links));
                Arc::clone(&snapshot.links)
            })
        };
        if let Some(links) = new_links {
            self.live_preview.set_links(&self.note_text, links);
        }
        self.rebuild_sidebar();
        let mut selection = self.state.borrow().selection.clone();
        if !self.exists(&selection) {
            selection = SidebarItem::AllNotes;
            self.state.borrow_mut().selection = selection.clone();
        }
        self.highlight(&selection);
        self.refresh_list();
        if let Some(path) = changed {
            self.changed_on_disk(&path);
        }
        self.update_navigation();
        if probe::enabled() && !snapshot.rows.is_empty() && probe::act_once() {
            probe::sidebar_mode_requested(&self.window);
            probe::search_requested(&self.window);
            probe::toggle_sidebar_requested(&self.window);
            if let Some(item) = probe::requested_selection() {
                self.highlight(&item);
                self.select(item);
            }
            if let Some(option) = probe::requested_option() {
                self.choose(option);
            }
            if probe::new_note_requested() {
                probe::type_requested(&self.note_text);
            }
            for (i, path) in probe::requested_notes().iter().enumerate() {
                if let Some(row) = self.list.row_of(path) {
                    // Selecting the row goes through the same delegate path as a click.
                    self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                }
                if i == 0 {
                    probe::type_requested(&self.note_text);
                    probe::keys_requested(&self.note_text);
                    if let Some(row) = std::env::var("KNOTES_PROBE_COMPLETION_CLICK").ok().and_then(|r| r.parse().ok()) {
                        self.completion.click_row(row);
                    }
                }
            }
            probe::save_requested();
            probe::write_requested(self);
            if let (Some(name), Some(open)) = (probe::rename_requested(), self.open_path()) {
                self.rename_note(&open, &name);
            }
            probe::list_menu_requested(&self.list, &self.table);
            probe::drag_requested(&self.list, &self.table, &self.sidebar);
            probe::folder_rename_requested(&self.sidebar, &self.outline);
            probe::toolbar_rename_requested(&self.window);
            if let Some(forward) = probe::requested_navigation() {
                self.navigate(forward);
            }
            if let Some(width) = std::env::var("KNOTES_PROBE_WIDTH").ok().and_then(|w| w.parse::<f64>().ok()) {
                let height = self.window.frame().size.height;
                self.window.setContentSize(objc2_foundation::NSSize::new(width, height));
                self.window.layoutIfNeeded();
            }
            if let Ok(cursor) = std::env::var("KNOTES_PROBE_CURSOR") {
                let end = self.note_text.string().length();
                let at = if cursor == "end" { end } else { cursor.parse().unwrap_or(0).min(end) };
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(at, 0));
            }
            probe::paste_requested(&self.note_text);
            probe::image_edge_requested(&self.note_text);
            if let Some(line) = std::env::var("KNOTES_PROBE_QUICK_LOOK").ok().and_then(|l| l.parse().ok()) {
                // The file the panel would show; the panel itself is not opened in probe mode.
                println!("quick-look: {}", self.note_text.image_file(line).map(|f| f.display().to_string()).unwrap_or_default());
            }
            if let Some((line, width)) = probe::resize_requested() {
                self.live_preview.resize_image(&self.note_text, line, width, true);
            }
            probe::menu_requested();
            if std::env::var("KNOTES_PROBE_SHORTCUT_FOCUS").as_deref() == Ok("list") {
                probe::shortcut_requested(&self.window, &self.table);
            } else {
                probe::shortcut_requested(&self.window, &self.note_text);
            }
            if let Ok(word) = std::env::var("KNOTES_PROBE_FIND") {
                self.open_find();
                self.find_bar.set_text(&word);
                self.find_text(&word, true);
                let steps: usize = std::env::var("KNOTES_PROBE_FIND_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                for _ in 0..steps {
                    self.find_step(true);
                }
            }
            probe::click_requested(&self.note_text);
            println!("found-in-note: {}", self.live_preview.found_count());
            if std::env::var("KNOTES_PROBE_APPEARANCE").as_deref() == Ok("dark") {
                // SAFETY: AppKit's appearance name constant lives for the whole process.
                objc2_app_kit::NSAppearanceCustomization::setAppearance(&*self.window, objc2_app_kit::NSAppearance::appearanceNamed(unsafe { objc2_app_kit::NSAppearanceNameDarkAqua }).as_deref());
            }
            if let Some(at) = std::env::var("KNOTES_PROBE_HOVER").ok().and_then(|a| a.parse().ok()) {
                println!("hover: {}", if self.live_preview.clickable(at) { "hand" } else { "text" });
            }
            probe::note_scroll_requested(&self.note_text);
            probe::schedule(true);
        }
    }

    /// Shows a note in the note column; `record` adds it to the back/forward history.
    pub fn open_note(&self, path: &str, record: bool) {
        self.leave();
        self.completion.hide();
        let text = {
            let mut state = self.state.borrow_mut();
            if record {
                state.history.visit(path);
            }
            // The file, not the index: the index may not have caught up with the last save yet.
            state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(path).ok()).unwrap_or_default()
        };
        *self.editing.borrow_mut() = Some(Editing { path: path.to_owned(), document: Document::open(&text) });
        self.live_preview.show(&self.note_text, path, &text);
        self.show_found_words(&text);
        if self.find_bar.is_shown() {
            self.find_text(&self.find_bar.text(), true);
        }
        self.update_navigation();
    }

    /// Opened from a search of the notes' text: the words it found, highlighted, the first in
    /// view.
    fn show_found_words(&self, text: &str) {
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

    /// Follows a clicked link: opens the note it resolves to (creating a missing one), then
    /// moves to its `#heading` or `^block`; external links open in their app (ADR 0013).
    fn follow_link(&self, link: &Link) {
        if link.kind == LinkKind::External {
            open_url(&link.target);
            return;
        }
        let Some(source) = self.open_path() else { return };
        let Some(links) = self.state.borrow().links.clone() else { return };
        let path = match links.resolve(link.kind, &link.target, &source) {
            Some(path) => path,
            None => match self.create_linked_note(link, &source) {
                Some(path) => path,
                None => return,
            },
        };
        if path != source {
            self.open_note(&path, true);
            self.highlight_open_note();
        }
        if let Some(subpath) = &link.subpath {
            let text = self.note_text.string().to_string();
            if let Some(at) = subpath_offset(&text, subpath) {
                self.note_text.move_to_top(utf16_offsets(&text, &[at])[0]);
            }
        }
    }

    /// Creates the empty note a link to a missing note names: at the written path for links
    /// with a folder (Markdown links: from the linking note's folder), else in the new notes
    /// folder. An existing file (not indexed yet) is opened as it is.
    fn create_linked_note(&self, link: &Link, source: &str) -> Option<String> {
        let target = link.target.trim().trim_start_matches('/');
        if target.is_empty() {
            return None;
        }
        let state = self.state.borrow();
        let vault = state.vault.clone()?;
        let folder = match link.kind {
            LinkKind::Markdown => source.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_owned()),
            _ if target.contains('/') => String::new(),
            _ => new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
        };
        drop(state);
        let mut path = normalize(&if folder.is_empty() { target.to_owned() } else { format!("{folder}/{target}") })?;
        if !path.to_lowercase().ends_with(".md") {
            path.push_str(".md");
        }
        let fs = FsVault::new(&vault);
        if !vault.join(&path).exists()
            && let Err(e) = fs.write_note(&path, "")
        {
            eprintln!("knotes: cannot create {path}: {e}");
            return None;
        }
        Some(path)
    }

    /// Shows, updates or hides the link completion list for the text before the cursor.
    fn update_completion(&self) {
        if self.accepting.get() {
            return;
        }
        let view = &self.note_text;
        let selection = view.selectedRange();
        let ns = view.string();
        let context = (selection.length == 0)
            .then(|| {
                let line = ns.paragraphRangeForRange(NSRange::new(selection.location, 0));
                let before = ns.substringWithRange(NSRange::new(line.location, selection.location - line.location)).to_string();
                link_completion(&before).map(|c| (line.location, before, c))
            })
            .flatten();
        let Some((line, before, completion)) = context else {
            self.completion.hide();
            self.completion_dismissed.set(None);
            return;
        };
        let units = |bytes: usize| line + before[..bytes].encode_utf16().count();
        let open = units(before.rfind("[[").unwrap_or(0));
        if self.completion_dismissed.get() == Some(open) {
            return;
        }
        let from = match &completion {
            LinkCompletion::Title { from, .. } | LinkCompletion::Heading { from, .. } | LinkCompletion::Block { from, .. } => units(*from),
        };
        let items = self.completion_items(&completion);
        if items.is_empty() {
            self.completion.hide();
            return;
        }
        self.completion_from.set(from);
        // SAFETY: the range lies inside the text; a null actual range is allowed.
        let cursor = unsafe { view.firstRectForCharacterRange_actualRange(NSRange::new(selection.location, 0), std::ptr::null_mut()) };
        self.completion.show(&self.window, cursor, items);
    }

    fn completion_items(&self, completion: &LinkCompletion) -> Vec<CompletionItem> {
        let state = self.state.borrow();
        let (Some(links), Some(titles), Some(source)) = (state.links.as_ref(), state.titles.as_ref(), self.open_path()) else {
            return Vec::new();
        };
        match completion {
            LinkCompletion::Title { query, .. } => {
                let paths: Vec<(String, Option<String>)> = if query.is_empty() {
                    recent_notes(&state.rows, MAX_ROWS).into_iter().map(|p| (p, None)).collect()
                } else {
                    titles.search_titles(query, MAX_ROWS).into_iter().map(|h| (h.path, h.alias)).collect()
                };
                paths
                    .into_iter()
                    .map(|(path, alias)| {
                        let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
                        let name = name.strip_suffix(".md").unwrap_or(name);
                        let link = links.link_text(&path, &source);
                        CompletionItem {
                            title: alias.clone().unwrap_or_else(|| name.to_owned()),
                            detail: folder.to_owned(),
                            insert: alias.map_or_else(|| link.clone(), |a| format!("{link}|{a}")),
                        }
                    })
                    .collect()
            }
            LinkCompletion::Heading { target, query, .. } | LinkCompletion::Block { target, query, .. } => {
                let Some(path) = links.resolve(LinkKind::Wiki, target, &source) else { return Vec::new() };
                let text = if path == source {
                    self.note_text.string().to_string()
                } else {
                    state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(&path).ok()).unwrap_or_default()
                };
                let query = query.to_lowercase();
                if matches!(completion, LinkCompletion::Block { .. }) {
                    // The block anchors of the target, each with the start of its line.
                    return MarkdownParser::new()
                        .parse(&text)
                        .anchors
                        .into_iter()
                        .filter(|a| a.name.to_lowercase().contains(&query))
                        .take(MAX_ROWS)
                        .map(|a| {
                            let start = text[..a.span.start as usize].rfind('\n').map_or(0, |n| n + 1);
                            let line: String = text[start..a.span.start as usize].trim().chars().take(60).collect();
                            CompletionItem { title: format!("^{}", a.name), detail: line, insert: format!("^{}", a.name) }
                        })
                        .collect();
                }
                note_headings(&text)
                    .into_iter()
                    .filter(|h| h.text.to_lowercase().contains(&query))
                    .take(MAX_ROWS)
                    .map(|h| CompletionItem { insert: h.text.clone(), title: h.text, detail: format!("H{}", h.level) })
                    .collect()
            }
        }
    }

    /// Keys while the list is open: Up/Down choose, Return/Tab insert, Esc closes.
    fn completion_command(&self, command: Sel) -> bool {
        if !self.completion.is_visible() {
            return false;
        }
        if command == sel!(moveDown:) {
            self.completion.move_selection(1);
        } else if command == sel!(moveUp:) {
            self.completion.move_selection(-1);
        } else if command == sel!(insertNewline:) || command == sel!(insertTab:) {
            self.accept_completion();
        } else if command == sel!(cancelOperation:) {
            self.completion.hide();
            let view = &self.note_text;
            let ns = view.string();
            let cursor = view.selectedRange().location;
            let line = ns.paragraphRangeForRange(NSRange::new(cursor, 0));
            let before = ns.substringWithRange(NSRange::new(line.location, cursor - line.location)).to_string();
            let open = before.rfind("[[").map(|b| line.location + before[..b].encode_utf16().count());
            self.completion_dismissed.set(open);
        } else {
            return false;
        }
        true
    }

    /// Replaces the typed part with the chosen link (or heading), closing it with `]]` unless
    /// the brackets already follow, and puts the cursor after them.
    fn accept_completion(&self) {
        let Some(item) = self.completion.chosen() else { return };
        self.completion.hide();
        let view = &self.note_text;
        let cursor = view.selectedRange().location;
        let from = self.completion_from.get().min(cursor);
        let ns = view.string();
        let closed = cursor + 2 <= ns.length() && ns.substringWithRange(NSRange::new(cursor, 2)).to_string() == "]]";
        let text = if closed { item.insert } else { format!("{}]]", item.insert) };
        self.accepting.set(true);
        view.setSelectedRange(NSRange::new(from, cursor - from));
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { NSTextInputClient::insertText_replacementRange(&***view, &NSString::from_str(&text), not_found) };
        let after = from + text.encode_utf16().count() + if closed { 2 } else { 0 };
        view.setSelectedRange(NSRange::new(after, 0));
        self.accepting.set(false);
    }

    /// `light` or `dark`: the colour formulas are typeset in (probe mode).
    pub fn formula_ink(&self) -> &'static str {
        self.live_preview.formula_ink()
    }

    /// Probe lines of what the note draws in place of hidden source, in view.
    pub fn describe_drawn(&self) -> Vec<String> {
        LivePreview::describe_drawn(&self.note_text)
    }

    pub fn describe_completion(&self) -> String {
        self.completion.describe()
    }

    /// Path of the note in the editor.
    pub fn open_path(&self) -> Option<String> {
        self.editing.borrow().as_ref().map(|e| e.path.clone())
    }

    pub fn vault(&self) -> Option<PathBuf> {
        self.state.borrow().vault.clone()
    }

    /// The open note's file as it is on disk now (probe mode).
    pub fn disk_text(&self) -> Option<String> {
        let path = self.open_path()?;
        let vault = self.state.borrow().vault.clone()?;
        FsVault::new(&vault).read_text(&path).ok()
    }

    /// Whether the editor has edits not written yet.
    pub fn is_dirty(&self) -> bool {
        self.editing.borrow().as_ref().is_some_and(|e| e.document.is_dirty())
    }

    /// A user edit: autosave once the user pauses.
    fn edited(&self) {
        match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.edited(),
            None => return,
        }
        let edit = self.edits.get() + 1;
        self.edits.set(edit);
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(AUTOSAVE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade()
                && window.edits.get() == edit
            {
                window.save();
            }
        });
    }

    /// File → New Note: an empty `Без назви.md` in the selected folder (the new notes folder
    /// for "All notes" and tags), opened with the cursor in it (ADR 0011, 0013).
    pub fn new_note(&self) {
        let (vault, folder) = {
            let state = self.state.borrow();
            let Some(vault) = state.vault.clone() else { return };
            let folder = match &state.selection {
                SidebarItem::Folder(path) => path.clone(),
                _ => new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
            };
            (vault, folder)
        };
        let path = unique_note_path(&folder, strings::UNTITLED, |p| vault.join(p).exists());
        if let Err(e) = FsVault::new(&vault).write_note(&path, "") {
            eprintln!("knotes: cannot create {path}: {e}");
            return;
        }
        self.open_note(&path, true);
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.document = Document::untitled();
        }
        self.highlight_open_note();
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Saves and names a new note after its first line: the user leaves the note (another
    /// note opens, the window closes, the app quits).
    pub fn leave(&self) {
        self.write_edits();
        self.name_new_note(true);
    }

    /// Autosave and File → Save; names a new note whose first line is finished.
    pub fn save(&self) {
        self.write_edits();
        self.name_new_note(false);
    }

    /// Renames the note at `old` to `name` in its folder (a number added when the name is
    /// taken), with the links to it (ADR 0019). False when nothing moved.
    pub fn rename_note(&self, old: &str, name: &str) -> bool {
        let (Some(stem), Some(vault)) = (note_file_stem(name), self.vault()) else { return false };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        // A change of case only is a rename of the same file.
        let same = |p: &str| p.to_lowercase() == old.to_lowercase();
        let new = unique_note_path(folder, &stem, |p| !same(p) && vault.join(p).exists());
        new != old && self.move_notes(Moves::from([(old.to_owned(), new)]))
    }

    /// Asks for a folder and moves the note at `path` there, with the links to it (ADR 0019).
    pub fn move_note_to_chosen_folder(&self, path: &str) {
        let folders = {
            let state = self.state.borrow();
            let mut list = Vec::new();
            let mut stack = vec![&*state.folders];
            while let Some(node) = stack.pop() {
                list.push(node.path.clone());
                stack.extend(node.children.iter().rev());
            }
            list
        };
        let name = knotes_app::title_of(path).to_owned();
        let Some(folder) = rename_dialog::ask_folder(objc2::MainThreadOnly::mtm(&*self.window), &name, &folders) else { return };
        self.move_note_to(path, &folder);
    }

    /// Moves the note at `path` into `folder` (`""`: the vault's root), with the links to it.
    pub fn move_note_to(&self, path: &str, folder: &str) {
        let Some(vault) = self.vault() else { return };
        let file = path.rsplit('/').next().unwrap_or(path);
        let wanted = if folder.is_empty() { file.to_owned() } else { format!("{folder}/{file}") };
        if wanted == path {
            return;
        }
        let new = unique_note_path(folder, knotes_app::title_of(file), |p| vault.join(p).exists());
        self.move_notes(Moves::from([(path.to_owned(), new)]));
    }

    /// Renames the folder at `folder` to `name` (a `/` in it becomes `-`), with the links into
    /// it (ADR 0019): the folder itself moves, attachments and all. False when nothing moved.
    pub fn rename_folder(&self, folder: &str, name: &str) -> bool {
        let name = name.trim().replace('/', "-");
        let Some(vault) = self.vault().filter(|_| !name.is_empty()) else { return false };
        let new = match folder.rsplit_once('/') {
            Some((parent, _)) => format!("{parent}/{name}"),
            None => name,
        };
        if new == folder || (new.to_lowercase() != folder.to_lowercase() && vault.join(&new).exists()) {
            return false;
        }
        let prefix = format!("{folder}/");
        let moves: Moves = self
            .state
            .borrow()
            .catalog
            .iter()
            .filter_map(|e| e.path.strip_prefix(&prefix).map(|rest| (e.path.clone(), format!("{new}/{rest}"))))
            .collect();
        if !self.relocate(moves, Some((folder, &new))) {
            return false;
        }
        // The sidebar keeps showing the folder (or the folder inside it) under its new name.
        let selected = match &self.state.borrow().selection {
            SidebarItem::Folder(path) if path == folder => Some(new.clone()),
            SidebarItem::Folder(path) => path.strip_prefix(&prefix).map(|rest| format!("{new}/{rest}")),
            _ => None,
        };
        if let Some(path) = selected {
            self.state.borrow_mut().selection = SidebarItem::Folder(path);
        }
        true
    }

    /// Moves notes (new path by old), asking first when links would change and rewriting them
    /// on yes (ADR 0019). The open note is saved first and follows its file. False when
    /// nothing moved.
    pub fn move_notes(&self, moves: Moves) -> bool {
        self.relocate(moves, None)
    }

    /// `move_notes`, the files moved one by one, or all at once by moving `folder` (from, to).
    fn relocate(&self, moves: Moves, folder: Option<(&str, &str)>) -> bool {
        let Some(vault_dir) = self.vault() else { return false };
        self.write_edits();
        let plan = self.relinks(&moves);
        let update = plan.links == 0
            || match rename_dialog::ask_link_update(objc2::MainThreadOnly::mtm(&*self.window), plan.links, plan.edits.len()) {
                LinkUpdate::Update => true,
                LinkUpdate::Keep => false,
                LinkUpdate::Cancel => return false,
            };
        let vault = FsVault::new(&vault_dir);
        let mut moved = Moves::new();
        if let Some((from, to)) = folder {
            match vault.move_note(from, to) {
                Ok(()) => moved = moves.clone(),
                Err(e) => {
                    eprintln!("knotes: cannot move {from} to {to}: {e}");
                    return false;
                }
            }
        }
        for (from, to) in moves.iter().filter(|_| folder.is_none()) {
            match vault.move_note(from, to) {
                Ok(()) => {
                    moved.insert(from.clone(), to.clone());
                }
                Err(e) => eprintln!("knotes: cannot move {from} to {to}: {e}"),
            }
        }
        if moved.is_empty() && folder.is_none() {
            return false;
        }
        let failed = |path: &str| moves.iter().any(|(from, to)| to == path && !moved.contains_key(from));
        let edits: Vec<&(String, String)> = if update { plan.edits.iter().filter(|(path, _)| !failed(path)).collect() } else { Vec::new() };
        for (path, text) in &edits {
            if let Err(e) = vault.write_note(path, text) {
                eprintln!("knotes: cannot update links in {path}: {e}");
            }
        }
        for (from, to) in &moved {
            self.state.borrow_mut().history.rename(from, to);
        }
        if let Some(open) = self.open_path() {
            let path = moved.get(&open).cloned().unwrap_or(open);
            let edited = edits.iter().find(|(p, _)| *p == path).map(|(_, text)| text.clone());
            if let Some(editing) = self.editing.borrow_mut().as_mut() {
                editing.path = path.clone();
                if let Some(text) = &edited {
                    editing.document.saved(text);
                }
            }
            self.live_preview.set_path(&path);
            if let Some(text) = edited {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, &path, &text);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
        }
        self.update_navigation();
        true
    }

    /// The link edits `moves` call for, worked out from the index and the files.
    fn relinks(&self, moves: &Moves) -> Relinks {
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        let (Some(reader), Some(before), Some(vault)) = (state.reader.as_ref(), state.links.clone(), state.vault.clone()) else {
            return Relinks::default();
        };
        let after = LinkTargets::new(state.catalog.iter().map(|e| moves.get(&e.path).map_or(e.path.as_str(), String::as_str)));
        let files = FsVault::new(&vault);
        plan_relinks(moves, reader, &|p| files.read_text(p).ok(), &MarkdownParser::new(), &*before, &after).unwrap_or_else(|e| {
            eprintln!("knotes: cannot plan link updates: {e}");
            Relinks::default()
        })
    }

    /// Renames a new note's file after its first line, once (ADR 0011).
    fn name_new_note(&self, leaving: bool) {
        let text = self.note_text.string().to_string();
        let old = match self.editing.borrow().as_ref() {
            Some(editing) if editing.document.wants_name(&text, leaving) => editing.path.clone(),
            _ => return,
        };
        let Some(stem) = note_file_stem(text.lines().next().unwrap_or("")) else { return };
        let Some(vault) = self.vault() else { return };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        let path = unique_note_path(folder, &stem, |p| p != old && vault.join(p).exists());
        if path != old
            && let Err(e) = FsVault::new(&vault).move_note(&old, &path)
        {
            eprintln!("knotes: cannot rename {old} to {path}: {e}");
            return;
        }
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.path = path.clone();
            editing.document.named();
        }
        self.state.borrow_mut().history.rename(&old, &path);
        self.live_preview.set_path(&path);
    }

    /// Writes the editor's text if it has unsaved edits. A change on disk the window has not
    /// seen yet is kept next to the note as a conflict copy instead of being overwritten.
    fn write_edits(&self) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let copy = {
            let mut editing = self.editing.borrow_mut();
            let Some(editing) = editing.as_mut().filter(|e| e.document.is_dirty()) else { return };
            let vault = FsVault::new(&vault);
            let text = self.note_text.string().to_string();
            let mut copy = None;
            if let Ok(disk) = vault.read_text(&editing.path) {
                match editing.document.on_disk(&disk, &text) {
                    DiskChange::Conflict => copy = keep_conflict_copy(&vault, &editing.path, &disk),
                    DiskChange::Ignore if !editing.document.is_dirty() => return,
                    _ => {}
                }
            }
            write(&vault, editing, &text);
            copy.map(|copy| (editing.path.clone(), copy))
        };
        if let Some((path, copy)) = copy {
            self.tell_conflict(&path, &copy);
        }
    }

    /// Tells where the other version of a note went (ADR 0011), in a sheet on the window that
    /// can open it.
    fn tell_conflict(&self, path: &str, copy: &str) {
        if probe::enabled() {
            println!("conflict-notice: {copy}");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(strings::CONFLICT_TITLE));
        alert.setInformativeText(&NSString::from_str(&strings::conflict_details(knotes_app::title_of(path), knotes_app::title_of(copy))));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OPEN_COPY));
        let weak = self.this.borrow().clone();
        let copy = copy.to_owned();
        let handler = block2::RcBlock::new(move |response: NSModalResponse| {
            if response == NSAlertSecondButtonReturn
                && let Some(window) = weak.upgrade()
            {
                window.open_note(&copy, true);
            }
        });
        alert.beginSheetModalForWindow_completionHandler(&self.window, Some(&handler));
    }

    /// The open note changed on disk: our own save is ignored, a clean editor reloads, and
    /// edits on both sides keep both versions.
    fn changed_on_disk(&self, path: &str) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let Ok(disk) = FsVault::new(&vault).read_text(path) else { return };
        let text = self.note_text.string().to_string();
        let change = match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.on_disk(&disk, &text),
            None => return,
        };
        match change {
            DiskChange::Ignore => {}
            DiskChange::Reload => {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, path, &disk);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
            DiskChange::Conflict => {
                // The editor's text keeps the note's name; the copy holds the other side.
                let vault = FsVault::new(&vault);
                let copy = keep_conflict_copy(&vault, path, &disk);
                if let Some(editing) = self.editing.borrow_mut().as_mut() {
                    write(&vault, editing, &text);
                }
                if let Some(copy) = copy {
                    self.tell_conflict(path, &copy);
                }
            }
        }
    }

    /// Opens the current note again (editor bench).
    pub fn reopen_current(&self) {
        let current = self.state.borrow().history.current().map(str::to_owned);
        if let Some(path) = current {
            self.open_note(&path, false);
        }
    }

    pub fn live_preview_restyle(&self, source: &str) {
        self.live_preview.restyle_all(&self.note_text, source);
    }

    /// Back (`false`) or forward (`true`) in the note history; the list selection follows.
    pub fn navigate(&self, forward: bool) {
        let target = {
            let mut state = self.state.borrow_mut();
            let next = if forward { state.history.forward() } else { state.history.back() };
            next.map(str::to_owned)
        };
        if let Some(path) = target {
            self.open_note(&path, false);
            self.highlight_open_note();
        }
    }

    fn update_navigation(&self) {
        let state = self.state.borrow();
        self.toolbar_delegate.set_history(state.history.can_back(), state.history.can_forward());
        drop(state);
        let name = self.open_path().map(|p| knotes_app::title_of(&p).to_owned()).unwrap_or_default();
        self.toolbar_delegate.set_note_title(&name);
    }

    /// The open note and history availability, for probe mode.
    pub fn describe_note(&self) -> (Option<String>, bool, bool) {
        let state = self.state.borrow();
        (state.history.current().map(str::to_owned), state.history.can_back(), state.history.can_forward())
    }

    fn ensure_reader(&self, state: &mut State) {
        if state.reader.is_none() {
            state.reader = state.index_dir.as_deref().and_then(|dir| SqliteIndex::open_reader(dir).ok());
        }
    }

    /// Shows the notes of a sidebar item.
    pub fn select(&self, item: SidebarItem) {
        if self.highlighting.get() {
            return;
        }
        self.state.borrow_mut().selection = item;
        *self.search_results.borrow_mut() = None;
        self.refresh_list();
        if !self.state.borrow().search.trim().is_empty() {
            self.request_search();
        }
    }

    fn refresh_list(&self) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        let (title, listed) = match &selection {
            SidebarItem::Folder(path) => (path.rsplit('/').next().unwrap_or(path).to_owned(), None),
            SidebarItem::Tag(tag) => (format!("#{tag}"), Some(self.notes_with_tag(&mut state, tag))),
            SidebarItem::Untagged => (strings::UNTAGGED.to_owned(), Some(self.untagged_notes(&mut state))),
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
        let results = self.search_results.borrow();
        let items = match results.as_ref().filter(|_| !state.search.trim().is_empty()) {
            Some(results) => search_list(&state.rows, &filter, &settings, &results.titles, &results.content),
            None => build_note_list(&state.rows, &filter, &settings, &note_list_view::now()),
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
        self.window.setSubtitle(&NSString::from_str(&strings::notes_count(count, &formatted.to_string())));
    }

    /// Selects the open note's row (it moves when the note is saved), or none if it is not listed.
    fn highlight_open_note(&self) {
        self.highlighting.set(true);
        match self.open_path().and_then(|p| self.list.row_of(&p)) {
            Some(row) => self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false),
            // SAFETY: a nil sender is allowed.
            None => unsafe { self.table.deselectAll(None) },
        }
        self.highlighting.set(false);
    }

    /// The files/content switch: rewrites the query's operator and searches again.
    pub fn set_search_mode(&self, mode: SearchMode) {
        let text = with_search_mode(&self.search_bar.field.stringValue().to_string(), mode);
        self.search_bar.set_text(&text);
        self.search(&text);
    }

    /// Edit → Search Notes (Cmd+Shift+F): the search row shown, its field focused.
    pub fn show_search(&self) {
        self.search_bar.show(true);
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
    fn open_find(&self) {
        self.find_bar.show(true);
        let text = self.find_bar.text();
        if !text.is_empty() {
            self.find_text(&text, true);
        }
    }

    /// Done or Escape: the bar and its highlights go, the note gets the focus back.
    pub fn close_find(&self) {
        self.find_bar.show(false);
        self.live_preview.clear_found(&self.note_text);
        self.find_matches.borrow_mut().clear();
        self.find_current.set(None);
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Finds `query` in the note: every occurrence highlighted; with `jump`, the first one from
    /// the cursor on selected and in view (an edit refinds in place).
    fn find_text(&self, query: &str, jump: bool) {
        let text = self.note_text.string().to_string();
        let found = knotes_app::find_all(&text, query);
        let bounds: Vec<usize> = found.iter().flat_map(|r| [r.start, r.end]).collect();
        let units = utf16_offsets(&text, &bounds);
        let matches: Vec<(usize, usize)> = units.chunks(2).map(|p| (p[0], p[1])).collect();
        self.live_preview.highlight(&self.note_text, &matches);
        let cursor = self.note_text.selectedRange().location;
        let current = (!matches.is_empty()).then(|| matches.iter().position(|m| m.0 >= cursor).unwrap_or(0));
        *self.find_matches.borrow_mut() = matches;
        self.find_current.set(current);
        if jump && let Some(i) = current {
            self.select_match(i);
        }
        self.find_bar.show_count(self.find_current.get(), self.find_matches.borrow().len(), !query.is_empty());
    }

    /// The next (or previous) occurrence, round the note.
    fn find_step(&self, forward: bool) {
        let n = self.find_matches.borrow().len();
        if n == 0 {
            return;
        }
        let i = match self.find_current.get() {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None => 0,
        };
        self.find_current.set(Some(i));
        self.select_match(i);
        self.find_bar.show_count(Some(i), n, true);
    }

    /// Selects occurrence `i`, scrolls it into view and points at it.
    fn select_match(&self, i: usize) {
        let Some(&(from, to)) = self.find_matches.borrow().get(i) else { return };
        let range = objc2_foundation::NSRange::new(from, to - from);
        self.note_text.setSelectedRange(range);
        self.note_text.scrollRangeToVisible(range);
        self.note_text.showFindIndicatorForRange(range);
    }

    /// The magnifying glass: shows the search row, or hides it and drops the search.
    pub fn toggle_search(&self) {
        let shown = !self.search_bar.is_shown();
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
    fn request_search(&self) {
        let generation = self.search_generation.get() + 1;
        self.search_generation.set(generation);
        let (query, scope, titles, index_dir) = {
            let state = self.state.borrow();
            let filter_all = matches!(state.selection, SidebarItem::AllNotes);
            let scope = (!filter_all).then(|| self.list.paths());
            (state.search.clone(), scope, state.titles.clone(), state.index_dir.clone())
        };
        let (mode, query) = {
            let (mode, text) = parse_search(&query);
            (mode, text.to_owned())
        };
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
            worker.request(SearchRequest { generation, mode, query, scope, titles });
        }
    }

    /// Results from the search thread: shown unless a newer query is on its way.
    fn apply_search(&self, response: SearchResponse) {
        if response.generation == self.search_generation.get() && !self.state.borrow().search.trim().is_empty() {
            *self.search_results.borrow_mut() = Some(response);
            self.refresh_list();
        }
    }

    fn untagged_notes(&self, state: &mut State) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.untagged_notes().ok()).unwrap_or_default().into_iter().collect()
    }

    fn notes_with_tag(&self, state: &mut State, tag: &str) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    fn exists(&self, item: &SidebarItem) -> bool {
        let state = self.state.borrow();
        match item {
            // The inbox row shows before its folder exists.
            SidebarItem::Folder(path) => folder_exists(&state.folders, path) || *path == new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
            SidebarItem::Tag(tag) => tag_exists(&state.tags, tag),
            _ => true,
        }
    }

    /// Selects the row of `item` in the sidebar without notifying (the list follows `state`).
    fn highlight(&self, item: &SidebarItem) {
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

/// Paths of the `n` most recently modified notes, newest first.
fn recent_notes(rows: &[NoteRow], n: usize) -> Vec<String> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let n = n.min(order.len());
    if n == 0 {
        return Vec::new();
    }
    order.select_nth_unstable_by_key(n - 1, |&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.truncate(n);
    order.sort_by_key(|&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.into_iter().map(|i| rows[i].path.clone()).collect()
}

/// `path` with `.` and `..` segments applied; `None` if it leaves the vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Opens an external link in its app; probe mode only reports it.
fn open_url(url: &str) {
    if probe::enabled() {
        println!("open-url: {url}");
        return;
    }
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

fn write(vault: &FsVault, editing: &mut Editing, text: &str) {
    match vault.write_note(&editing.path, text) {
        Ok(()) => editing.document.saved(text),
        Err(e) => eprintln!("knotes: cannot save {}: {e}", editing.path),
    }
}

/// An image found on a pasteboard: a file already in the vault (its vault path), or one to
/// add to the attachments folder.
enum PastedImage {
    InVault(String),
    New { name: String, bytes: Vec<u8> },
}

/// The images a paste or a drop brings: image files (others are left), or else an image's
/// data alone; none when text comes with it (a cell copied from a table carries its picture
/// too, and the text is what is meant).
fn pasteboard_images(pasteboard: &NSPasteboard, vault: &Path) -> Vec<PastedImage> {
    // SAFETY: AppKit's pasteboard type constants live for the whole process.
    let (file_url, string, png, tiff) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardTypePNG, NSPasteboardTypeTIFF) };
    let files: Vec<PathBuf> = pasteboard
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.stringForType(file_url)?;
                    NSURL::URLWithString(&url)?.path().map(|p| PathBuf::from(p.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    if !files.is_empty() {
        let vaults = [vault.to_path_buf(), vault.canonicalize().unwrap_or_default()];
        return files
            .into_iter()
            .filter(|f| f.to_str().is_some_and(|f| FileKind::from_path(f) == FileKind::Image))
            .filter_map(|file| {
                let inside = vaults.iter().find_map(|v| file.strip_prefix(v).ok().and_then(Path::to_str).map(str::to_owned));
                match inside {
                    Some(path) => Some(PastedImage::InVault(path)),
                    None => Some(PastedImage::New { name: file.file_name()?.to_str()?.to_owned(), bytes: std::fs::read(&file).ok()? }),
                }
            })
            .collect();
    }
    if pasteboard.stringForType(string).is_some() {
        return Vec::new();
    }
    let bytes = pasteboard.dataForType(png).map(|d| d.to_vec()).or_else(|| {
        let data = pasteboard.dataForType(tiff)?;
        let rep = NSBitmapImageRep::imageRepWithData(&data)?;
        // SAFETY: an empty properties dictionary is valid for PNG.
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }.map(|d| d.to_vec())
    });
    let Some(bytes) = bytes else { return Vec::new() };
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyyMMddHHmmss")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    vec![PastedImage::New { name: pasted_image_name(&stamp), bytes }]
}

/// Writes the disk version of `path` as "Name (конфлікт date time).md" next to it.
fn keep_conflict_copy(vault: &FsVault, path: &str, disk: &str) -> Option<String> {
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    let mut copy = conflict_path(path, &stamp);
    if vault.read_text(&copy).is_ok() {
        formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm.ss")));
        copy = conflict_path(path, &formatter.stringFromDate(&NSDate::now()).to_string());
    }
    match vault.write_note(&copy, disk) {
        Ok(()) => Some(copy),
        Err(e) => {
            eprintln!("knotes: cannot keep conflict copy {copy}: {e}");
            None
        }
    }
}

/// Tags and "Untagged" have no folder and no `.knotes.json`: their list options last the session.
fn session_settings_key(item: &SidebarItem) -> Option<String> {
    matches!(item, SidebarItem::Tag(_) | SidebarItem::Untagged).then(|| item.key())
}

fn folder_exists(node: &FolderNode, path: &str) -> bool {
    node.path == path || node.children.iter().any(|c| folder_exists(c, path))
}

fn tag_exists(nodes: &[TagNode], tag: &str) -> bool {
    nodes.iter().any(|n| n.tag == tag || tag_exists(&n.children, tag))
}

pub fn split_items(
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
) -> [Retained<NSSplitViewItem>; 4] {
    [
        NSSplitViewItem::sidebarWithViewController(sidebar),
        NSSplitViewItem::contentListWithViewController(list),
        NSSplitViewItem::splitViewItemWithViewController(note),
        NSSplitViewItem::inspectorWithViewController(chat),
    ]
}

/// Minimum and maximum width and holding priority per column: when the window resizes, the
/// note column stretches and the others keep their width. Explicit maximums: the inspector
/// role caps itself at 270 points otherwise.
/// Narrowest list column while the sidebar is collapsed (see `VaultWindow::new`).
const COLLAPSED_LIST_MIN: f64 = 400.0;

const COLUMNS: [(f64, f64, f32); 4] = [(200.0, 480.0, 260.0), (260.0, 640.0, 255.0), (360.0, f64::MAX, 250.0), (260.0, 640.0, 255.0)];

pub fn split_controller(
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
    mtm: MainThreadMarker,
) -> Retained<NSSplitViewController> {
    let split = NSSplitViewController::new(mtm);
    for (item, (min, max, holding)) in split_items(sidebar, list, note, chat).iter().zip(COLUMNS) {
        item.setMinimumThickness(min);
        if max < f64::MAX {
            item.setMaximumThickness(max);
        }
        item.setHoldingPriority(holding);
        split.addSplitViewItem(item);
    }
    split
}
