//! A window showing one vault: owns the AppKit objects and applies session events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use knotes_app::{FolderNode, FolderRules, History, ListFilter, ListOption, ListSettings, Sidebar, SidebarItem, TagNode, build_note_list, resolve_list_settings};
use knotes_core::{FolderRulesStore, NoteRow, NoteText, TagIndex};
use knotes_index::SqliteIndex;
use knotes_session::{Event, Snapshot};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSTextView, NSViewController, NSWindow};
use objc2_foundation::{NSIndexSet, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString};

use crate::chat_view::{self, ChatView};
use crate::list_menu::ListMenu;
use crate::live_preview::LivePreview;
use crate::note_list_view::{self, NoteListSource};
use crate::note_view;
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
    index_dir: Option<PathBuf>,
    vault: Option<PathBuf>,
    /// List settings of tag views: tags have no folder and no `.knotes`; kept for the session.
    tag_settings: HashMap<String, ListSettings>,
    history: History,
    /// Read-only index connection, opened on first need (the index exists after the first snapshot).
    reader: Option<SqliteIndex>,
}

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
    pub list_menu: ListMenu,
    pub note_text: Retained<NSTextView>,
    /// Delegate of `note_text`, kept alive here.
    live_preview: Retained<LivePreview>,
    pub chat: ChatView,
    /// Kept alive here: the toolbar holds its delegate weakly.
    toolbar_delegate: Retained<ToolbarDelegate>,
    state: RefCell<State>,
    /// Set while the window selects a sidebar row itself, so the selection callback ignores it.
    highlighting: Cell<bool>,
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Rc<Self> {
        let empty = Sidebar::new(&Default::default(), &[]);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
        let list = NoteListSource::new(mtm);
        let (list_scroll, table) = note_list_view::table(mtm, &list);
        let list_vc = NSViewController::new(mtm);
        list_vc.setView(&list_scroll);
        let (note_scroll, note_text) = note_view::note_view(mtm);
        let live_preview = LivePreview::new(mtm);
        note_text.setDelegate(Some(ProtocolObject::from_ref(&*live_preview)));
        live_preview.attach(&note_text);
        let note_vc = NSViewController::new(mtm);
        note_vc.setView(&note_scroll);
        let chat = chat_view::chat_view(mtm);
        let chat_vc = NSViewController::new(mtm);
        chat_vc.setView(&chat.view);
        let (window, split) = window::build(mtm, &sidebar_vc, &list_vc, &note_vc, &chat_vc);
        let list_menu = ListMenu::new(mtm);
        let toolbar = ToolbarDelegate::new(mtm, split.splitView(), list_menu.menu.clone());
        toolbar::install(&window, &toolbar);
        let state = State {
            selection: SidebarItem::AllNotes,
            rows: Arc::new(Vec::new()),
            rules: Arc::new(HashMap::new()),
            folders: Arc::new(FolderNode::default()),
            tags: Arc::new(Vec::new()),
            index_dir: None,
            vault: None,
            tag_settings: HashMap::new(),
            history: History::default(),
            reader: None,
        };
        let this = Rc::new(Self {
            window,
            sidebar,
            outline,
            list,
            table,
            list_menu,
            note_text,
            live_preview,
            chat,
            toolbar_delegate: toolbar,
            state: RefCell::new(state),
            highlighting: Cell::new(false),
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_select(move |item| {
            if let Some(window) = weak.upgrade() {
                window.select(item);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_select(move |path| {
            if let Some(window) = weak.upgrade() {
                window.open_note(&path, true);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
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

    pub fn set_vault(&self, vault: PathBuf, index_dir: PathBuf) {
        let mut state = self.state.borrow_mut();
        state.vault = Some(vault);
        state.index_dir = Some(index_dir);
    }

    /// Applies a list options choice: saved in the folder's `.knotes` (only that field), or
    /// kept for the session for tag views.
    pub fn choose(&self, option: ListOption) {
        let mut state = self.state.borrow_mut();
        match state.selection.clone() {
            SidebarItem::Tag(tag) => {
                let current = state.tag_settings.get(&tag).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules));
                state.tag_settings.insert(tag, current.with(option));
            }
            selection => {
                let folder = match &selection {
                    SidebarItem::Folder(path) => path.clone(),
                    _ => String::new(),
                };
                let mut rules = state.rules.get(&folder).cloned().unwrap_or_else(|| FolderRules::parse("{}").expect("empty object"));
                rules.set_option(option);
                if let Some(vault) = &state.vault
                    && let Err(e) = FsVault::new(vault).write_rules(&folder, &rules.to_json())
                {
                    eprintln!("knotes: cannot save {folder}/.knotes: {e}");
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
        let reopen = {
            let mut state = self.state.borrow_mut();
            let paths: HashSet<&str> = snapshot.rows.iter().map(|r| r.path.as_str()).collect();
            state.history.retain(|p| paths.contains(p));
            let current = state.history.current().map(str::to_owned);
            current.filter(|c| snapshot.changed.iter().any(|p| p == c))
        };
        {
            let mut state = self.state.borrow_mut();
            state.rows = Arc::clone(&snapshot.rows);
            state.rules = Arc::clone(&snapshot.rules);
            state.folders = Arc::clone(&snapshot.folders);
            state.tags = Arc::clone(&snapshot.tags);
        }
        self.sidebar.set_model(Sidebar::new(&snapshot.folders, &snapshot.tags));
        self.outline.reloadData();
        for section in [SidebarItem::FoldersSection, SidebarItem::TagsSection] {
            // SAFETY: section objects come from the data source of this outline view.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&section))) };
        }
        let mut selection = self.state.borrow().selection.clone();
        if !self.exists(&selection) {
            selection = SidebarItem::AllNotes;
            self.state.borrow_mut().selection = selection.clone();
        }
        self.highlight(&selection);
        self.refresh_list();
        if let Some(path) = reopen {
            self.open_note(&path, false);
        }
        self.update_navigation();
        if probe::enabled() && !snapshot.rows.is_empty() {
            if let Some(item) = probe::requested_selection() {
                self.highlight(&item);
                self.select(item);
            }
            if let Some(option) = probe::requested_option() {
                self.choose(option);
            }
            for path in probe::requested_notes() {
                if let Some(row) = self.list.row_of(&path) {
                    // Selecting the row goes through the same delegate path as a click.
                    self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                }
            }
            if let Some(forward) = probe::requested_navigation() {
                self.navigate(forward);
            }
            if let Some(width) = std::env::var("KNOTES_PROBE_WIDTH").ok().and_then(|w| w.parse::<f64>().ok()) {
                let height = self.window.frame().size.height;
                self.window.setContentSize(objc2_foundation::NSSize::new(width, height));
                self.window.layoutIfNeeded();
            }
            if std::env::var("KNOTES_PROBE_CURSOR").as_deref() == Ok("end") {
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(end, 0));
            }
            probe::schedule(true);
        }
    }

    /// Shows a note in the note column; `record` adds it to the back/forward history.
    pub fn open_note(&self, path: &str, record: bool) {
        let text = {
            let mut state = self.state.borrow_mut();
            if record {
                state.history.visit(path);
            }
            self.ensure_reader(&mut state);
            state.reader.as_ref().and_then(|r| r.note_text(path).ok().flatten()).unwrap_or_default()
        };
        self.live_preview.show(&self.note_text, &text);
        self.update_navigation();
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
            if let Some(row) = self.list.row_of(&path) {
                self.highlighting.set(true);
                self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                self.highlighting.set(false);
            }
        }
    }

    fn update_navigation(&self) {
        let state = self.state.borrow();
        self.toolbar_delegate.set_history(state.history.can_back(), state.history.can_forward());
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
        self.refresh_list();
    }

    fn refresh_list(&self) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        let (title, tagged) = match &selection {
            SidebarItem::Folder(path) => (path.rsplit('/').next().unwrap_or(path).to_owned(), None),
            SidebarItem::Tag(tag) => (format!("#{tag}"), Some(self.notes_with_tag(&mut state, tag))),
            _ => (strings::ALL_NOTES.to_owned(), None),
        };
        let filter = match (&selection, &tagged) {
            (SidebarItem::Folder(path), _) => ListFilter::Folder(path),
            (SidebarItem::Tag(_), Some(paths)) => ListFilter::Paths(paths),
            _ => ListFilter::All,
        };
        let folder = match &selection {
            SidebarItem::Folder(path) => path.as_str(),
            _ => "",
        };
        let settings = match &selection {
            SidebarItem::Tag(tag) => state.tag_settings.get(tag).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules)),
            _ => resolve_list_settings(folder, &state.rules),
        };
        self.list_menu.update(&settings, !matches!(selection, SidebarItem::Tag(_)));
        let items = build_note_list(&state.rows, &filter, &settings, &note_list_view::now());
        self.list.set_items(Arc::clone(&state.rows), items);
        drop(state);
        self.table.reloadData();
        self.window.setTitle(&NSString::from_str(&title));
        let count = self.list.note_count();
        let formatted = NSNumberFormatter::localizedStringFromNumber_numberStyle(
            &NSNumber::numberWithUnsignedLongLong(count as u64),
            NSNumberFormatterStyle::DecimalStyle,
        );
        self.window.setSubtitle(&NSString::from_str(&strings::notes_count(count, &formatted.to_string())));
    }

    fn notes_with_tag(&self, state: &mut State, tag: &str) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    fn exists(&self, item: &SidebarItem) -> bool {
        let state = self.state.borrow();
        match item {
            SidebarItem::Folder(path) => folder_exists(&state.folders, path),
            SidebarItem::Tag(tag) => tag_exists(&state.tags, tag),
            _ => true,
        }
    }

    /// Selects the row of `item` in the sidebar without notifying (the list follows `state`).
    fn highlight(&self, item: &SidebarItem) {
        // SAFETY: the item object comes from this outline view's data source.
        let row = unsafe { self.outline.rowForItem(Some(&self.sidebar.object_for(item))) };
        if row >= 0 {
            self.highlighting.set(true);
            self.outline.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row as usize), false);
            self.highlighting.set(false);
        }
    }
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
