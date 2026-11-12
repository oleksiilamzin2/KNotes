//! A window showing one vault: owns the AppKit objects and applies session events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};

use knotes_app::{FolderNode, FolderRules, History, ListFilter, ListOption, ListSettings, Sidebar, SidebarItem, TagNode, build_note_list, resolve_list_settings};
use knotes_core::{FolderRulesStore, NoteRow, NoteWriter, TagIndex, VaultReader};
use knotes_editor::{DiskChange, Document, conflict_path};
use knotes_index::SqliteIndex;
use knotes_session::{Event, Snapshot};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSTextView, NSViewController, NSWindow};
use objc2_foundation::{NSDate, NSDateFormatter, NSIndexSet, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString};

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
    pub note_text: Retained<NSTextView>,
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
            editing: RefCell::new(None),
            edits: Cell::new(0),
            this: RefCell::new(Weak::new()),
        });
        *this.this.borrow_mut() = Rc::downgrade(&this);
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_change(move || {
            if let Some(window) = weak.upgrade() {
                window.edited();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_select(move |item| {
            if let Some(window) = weak.upgrade() {
                window.select(item);
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
        {
            let mut state = self.state.borrow_mut();
            let paths: HashSet<&str> = snapshot.rows.iter().map(|r| r.path.as_str()).collect();
            state.history.retain(|p| paths.contains(p));
        }
        let changed = self.open_path().filter(|open| snapshot.changed.iter().any(|p| p == open));
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
        if let Some(path) = changed {
            self.changed_on_disk(&path);
        }
        self.update_navigation();
        if probe::enabled() && !snapshot.rows.is_empty() && probe::act_once() {
            if let Some(item) = probe::requested_selection() {
                self.highlight(&item);
                self.select(item);
            }
            if let Some(option) = probe::requested_option() {
                self.choose(option);
            }
            for (i, path) in probe::requested_notes().iter().enumerate() {
                if let Some(row) = self.list.row_of(path) {
                    // Selecting the row goes through the same delegate path as a click.
                    self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                }
                if i == 0 {
                    probe::type_requested(&self.note_text);
                }
            }
            probe::save_requested();
            probe::write_requested(self);
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
            probe::schedule(true);
        }
    }

    /// Shows a note in the note column; `record` adds it to the back/forward history.
    pub fn open_note(&self, path: &str, record: bool) {
        self.save();
        let text = {
            let mut state = self.state.borrow_mut();
            if record {
                state.history.visit(path);
            }
            // The file, not the index: the index may not have caught up with the last save yet.
            state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(path).ok()).unwrap_or_default()
        };
        *self.editing.borrow_mut() = Some(Editing { path: path.to_owned(), document: Document::open(&text) });
        self.live_preview.show(&self.note_text, &text);
        self.update_navigation();
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

    /// Writes the editor's text if it has unsaved edits. A change on disk the window has not
    /// seen yet is kept next to the note as a conflict copy instead of being overwritten.
    pub fn save(&self) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let mut editing = self.editing.borrow_mut();
        let Some(editing) = editing.as_mut().filter(|e| e.document.is_dirty()) else { return };
        let vault = FsVault::new(&vault);
        let text = self.note_text.string().to_string();
        if let Ok(disk) = vault.read_text(&editing.path) {
            match editing.document.on_disk(&disk, &text) {
                DiskChange::Conflict => keep_conflict_copy(&vault, &editing.path, &disk),
                DiskChange::Ignore if !editing.document.is_dirty() => return,
                _ => {}
            }
        }
        write(&vault, editing, &text);
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
                self.live_preview.show(&self.note_text, &disk);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
            DiskChange::Conflict => {
                // The editor's text keeps the note's name; the copy holds the other side.
                let vault = FsVault::new(&vault);
                keep_conflict_copy(&vault, path, &disk);
                if let Some(editing) = self.editing.borrow_mut().as_mut() {
                    write(&vault, editing, &text);
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

fn write(vault: &FsVault, editing: &mut Editing, text: &str) {
    match vault.write_note(&editing.path, text) {
        Ok(()) => editing.document.saved(text),
        Err(e) => eprintln!("knotes: cannot save {}: {e}", editing.path),
    }
}

/// Writes the disk version of `path` as "Name (конфлікт date time).md" next to it.
fn keep_conflict_copy(vault: &FsVault, path: &str, disk: &str) {
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    let mut copy = conflict_path(path, &stamp);
    if vault.read_text(&copy).is_ok() {
        formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm.ss")));
        copy = conflict_path(path, &formatter.stringFromDate(&NSDate::now()).to_string());
    }
    if let Err(e) = vault.write_note(&copy, disk) {
        eprintln!("knotes: cannot keep conflict copy {copy}: {e}");
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
